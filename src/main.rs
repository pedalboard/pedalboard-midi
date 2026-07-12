#![no_std]
#![no_main]

mod crash;
mod hmi;

use defmt_rtt as _;
#[cfg(debug_assertions)]
use panic_probe as _;
use rtic::app;

use rtic_monotonics::rp2040::prelude::*;

rp2040_timer_monotonic!(Mono);

/// The linker will place this boot block at the start of our program image. We
/// need this to help the ROM bootloader get our code up and running.
/// Note: This boot block is not necessary when using a rp-hal based BSP
/// as the BSPs already perform this step.
#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

/// External crystal on the pedalboard-hw board is 12 MHz.
const XTAL_FREQ_HZ: u32 = 12_000_000u32;

#[app(device = rp2040_hal::pac, dispatchers = [SW0_IRQ])]
mod app {
    use super::*;

    use crate::hmi::inputs::{Buttons, ExpressionPedals, Inputs, Rotary};
    use crate::Mono;
    use core::mem::MaybeUninit;
    use defmt::{debug, error, info, warn};
    use embedded_hal::digital::OutputPin;
    use embedded_hal_bus::i2c::AtomicDevice;
    use embedded_hal_bus::util::AtomicCell;
    use pedalboard_midi::leds::{Led, LedEvent};
    use pedalboard_midi::persist::PERSIST_CAPACITY;
    use pedalboard_midi::system_status::SystemStatus;
    use rtic_sync::channel::{Receiver, Sender};
    use rtic_sync::make_channel;

    use heapless::Vec;
    use midi_convert::midi_types::MidiMessage;
    use midi_convert::{parse::MidiTryParseSlice, render_slice::MidiRenderSlice};
    use rp2040_hal::{
        adc::{Adc, AdcPin},
        clocks::init_clocks_and_plls,
        fugit::{HertzU32, RateExtU32},
        gpio::{
            bank0::{
                Gpio0, Gpio1, Gpio10, Gpio11, Gpio16, Gpio17, Gpio18, Gpio19, Gpio2, Gpio20,
                Gpio21, Gpio24, Gpio25, Gpio3, Gpio4, Gpio5, Gpio6, Gpio7,
            },
            FunctionI2C, FunctionSio, FunctionUart, Pin, Pins, PullDown, PullUp, SioInput,
            SioOutput,
        },
        i2c::I2C,
        pac::{I2C0, UART0},
        pio::PIOExt,
        uart::{DataBits, Reader, StopBits, UartConfig, UartPeripheral, Writer},
        usb::UsbBus,
        Clock, Sio, Watchdog,
    };
    use smart_leds::{brightness, SmartLedsWrite};
    use usb_device::{
        class_prelude::UsbBusAllocator,
        device::{StringDescriptors, UsbDeviceBuilder, UsbVidPid},
        prelude::UsbDeviceState,
    };
    use usbd_midi::{CableNumber, UsbMidiClass, UsbMidiEventPacket, UsbMidiPacketReader};

    use ws2812_pio::Ws2812Direct;

    type MidiUartPins = (
        Pin<Gpio0, FunctionUart, PullDown>,
        Pin<Gpio1, FunctionUart, PullDown>,
    );
    type MidiOut = embedded_midi::MidiOut<Writer<UART0, MidiUartPins>>;
    type MidiIn = embedded_midi::MidiIn<Reader<UART0, MidiUartPins>>;

    pub type I2CBus = I2C<
        I2C0,
        (
            Pin<Gpio24, FunctionI2C, PullUp>,
            Pin<Gpio25, FunctionI2C, PullUp>,
        ),
    >;

    type LedPio = Ws2812Direct<
        rp2040_hal::pac::PIO0,
        rp2040_hal::pio::SM0,
        Pin<Gpio11, rp2040_hal::gpio::FunctionPio0, PullDown>,
    >;

    type DigInPin<P> = Pin<P, FunctionSio<SioInput>, PullUp>;
    pub type InputPins = Inputs<
        DigInPin<Gpio6>,
        DigInPin<Gpio5>,
        DigInPin<Gpio2>,
        DigInPin<Gpio7>,
        DigInPin<Gpio4>,
        DigInPin<Gpio3>,
        DigInPin<Gpio16>,
        DigInPin<Gpio17>,
        DigInPin<Gpio18>,
        DigInPin<Gpio19>,
        DigInPin<Gpio20>,
        DigInPin<Gpio21>,
    >;

    #[shared]
    struct Shared {
        usb_midi: UsbMidiClass<'static, UsbBus>,
        usb_dev: usb_device::device::UsbDevice<'static, UsbBus>,
        active_preset: u8,
        pe_config: midi_controller::config::Config,
        global_config: midi_controller::config::GlobalConfig,
        state_store: midi_controller::state::PresetStateStore,
        presets_skipped: u8,
        button_active: [bool; 6],
    }

    #[local]
    struct Local {
        uart_midi_out: MidiOut,
        uart_midi_in: MidiIn,
        inputs: InputPins,
        led_spi: LedPio,
        displays: crate::hmi::display::Displays<
            AtomicDevice<'static, I2CBus>,
            AtomicDevice<'static, I2CBus>,
        >,
        debug_led: Pin<Gpio10, FunctionSio<SioOutput>, PullDown>,
        led_sender_usb: Sender<'static, LedEvent, LED_CAPACITY>,
        usb_sender_usb_thru: Sender<'static, UsbMidiEventPacket, USB_OUT_CAPACITY>,
        din_thru_receiver: Receiver<'static, [u8; 3], DIN_THRU_CAPACITY>,
        din_thru_sender: Sender<'static, [u8; 3], DIN_THRU_CAPACITY>,
        trigger_sender_din: Sender<'static, [u8; 3], TRIGGER_CAPACITY>,
        trigger_sender_usb: Sender<'static, [u8; 3], TRIGGER_CAPACITY>,
        trigger_receiver: Receiver<'static, [u8; 3], TRIGGER_CAPACITY>,
        persist_sender: Sender<'static, pedalboard_midi::persist::PersistCommand, PERSIST_CAPACITY>,
        eeprom_i2c: AtomicDevice<'static, I2CBus>,
    }
    const USB_OUT_CAPACITY: usize = 128;
    const _: () = assert!(
        USB_OUT_CAPACITY >= pedalboard_midi::MIN_USB_OUT_CAPACITY,
        "USB_OUT_CAPACITY too small for MAX_PRESET_SIZE PE replies"
    );
    const DISPLAY_LOG_CAPACITY: usize = 8;
    const LED_CAPACITY: usize = 4;
    const DIN_THRU_CAPACITY: usize = 8;
    const TRIGGER_CAPACITY: usize = 8;
    const SYSTEM_STATUS_CAPACITY: usize = 1;
    const CONFIG_DISPLAY_CAPACITY: usize = 8;

    #[init(local = [
        usb_bus: MaybeUninit<usb_device::bus::UsbBusAllocator<UsbBus>> = MaybeUninit::uninit(),
        i2c_bus: MaybeUninit<AtomicCell<I2CBus>> = MaybeUninit::uninit(),
        adc: MaybeUninit<rp2040_hal::adc::Adc> = MaybeUninit::uninit()
    ])]
    fn init(ctx: init::Context) -> (Shared, Local) {
        // Check if we recovered from a crash (must be before any RAM init)
        crate::crash::check_crash_marker();

        let mut resets = ctx.device.RESETS;
        Mono::start(ctx.device.TIMER, &resets);
        let mut watchdog = Watchdog::new(ctx.device.WATCHDOG);

        let clocks = init_clocks_and_plls(
            crate::XTAL_FREQ_HZ,
            ctx.device.XOSC,
            ctx.device.CLOCKS,
            ctx.device.PLL_SYS,
            ctx.device.PLL_USB,
            &mut resets,
            &mut watchdog,
        )
        .ok()
        .unwrap();

        let sio = Sio::new(ctx.device.SIO);
        let pins = Pins::new(
            ctx.device.IO_BANK0,
            ctx.device.PADS_BANK0,
            sio.gpio_bank0,
            &mut resets,
        );

        let mut debug_led = pins.gpio10.into_push_pull_output();
        debug_led.set_high().unwrap();

        // Read flash unique ID for stable USB serial number
        let mut flash_id = [0u8; 8];
        unsafe {
            rp2040_flash::flash::flash_unique_id(&mut flash_id, true);
        }
        // Static storage for serial string (init runs once at boot, no races)
        #[allow(static_mut_refs)]
        let serial_str = unsafe {
            static mut BUF: [u8; 16] = [0u8; 16];
            for (i, byte) in flash_id.iter().enumerate() {
                let hi = byte >> 4;
                let lo = byte & 0x0F;
                BUF[i * 2] = if hi < 10 { b'0' + hi } else { b'A' + hi - 10 };
                BUF[i * 2 + 1] = if lo < 10 { b'0' + lo } else { b'A' + lo - 10 };
            }
            core::str::from_utf8_unchecked(&BUF)
        };

        // USB
        let usb_bus: &'static _ = ctx.local.usb_bus.write(UsbBusAllocator::new(UsbBus::new(
            ctx.device.USBCTRL_REGS,
            ctx.device.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut resets,
        )));

        let usb_midi = UsbMidiClass::new(usb_bus, 1, 1).unwrap();
        let usb_dev = UsbDeviceBuilder::new(usb_bus, UsbVidPid(0x2E8A, 0x0005))
            .strings(&[StringDescriptors::default()
                .product("pedalboard MIDI")
                .manufacturer("github.com/pedalboard")
                .serial_number(serial_str)])
            .expect("Failed to set usb device strings")
            .device_class(0)
            .device_sub_class(0)
            .build();

        // UART Midi
        let uart_pins = (
            pins.gpio0.into_function::<FunctionUart>(),
            pins.gpio1.into_function::<FunctionUart>(),
        );
        let conf = UartConfig::new(
            HertzU32::from_raw(31250),
            DataBits::Eight,
            None,
            StopBits::One,
        );
        let uart = UartPeripheral::new(ctx.device.UART0, uart_pins, &mut resets)
            .enable(conf, clocks.peripheral_clock.freq())
            .unwrap();
        let (mut rx, tx) = uart.split();
        rx.enable_rx_interrupt();
        let uart_midi_out = MidiOut::new(tx);
        let uart_midi_in = MidiIn::new(rx);

        // input pins
        let vol = Rotary::new(
            pins.gpio16.into_pull_up_input(),
            pins.gpio17.into_pull_up_input(),
            pins.gpio18.into_pull_up_input(),
        );

        let gain = Rotary::new(
            pins.gpio19.into_pull_up_input(),
            pins.gpio20.into_pull_up_input(),
            pins.gpio21.into_pull_up_input(),
        );

        let buttons = Buttons::new(
            pins.gpio6.into_pull_up_input(),
            pins.gpio5.into_pull_up_input(),
            pins.gpio2.into_pull_up_input(),
            pins.gpio7.into_pull_up_input(),
            pins.gpio4.into_pull_up_input(),
            pins.gpio3.into_pull_up_input(),
        );

        // ADC for analog input
        let adc = ctx.local.adc.write(Adc::new(ctx.device.ADC, &mut resets));

        let exp2_pin = AdcPin::new(pins.gpio27.into_floating_input()).unwrap();
        let exp1_pin = AdcPin::new(pins.gpio28.into_floating_input()).unwrap();
        let exp = ExpressionPedals::new_direct(adc, exp2_pin, exp1_pin);
        let inputs = Inputs::new(vol, gain, buttons, exp);

        // Configure PIO for Ws2812 LEDs
        let led_pin: Pin<Gpio11, rp2040_hal::gpio::FunctionPio0, PullDown> =
            pins.gpio11.into_function();
        let (mut pio0, sm0, _, _, _) = ctx.device.PIO0.split(&mut resets);
        let led_spi = Ws2812Direct::new(led_pin, &mut pio0, sm0, clocks.peripheral_clock.freq());

        // Configure I²C for OLED display
        let sda_pin: Pin<_, FunctionI2C, PullUp> = pins.gpio24.reconfigure();
        let scl_pin: Pin<_, FunctionI2C, PullUp> = pins.gpio25.reconfigure();
        let mut i2c = I2C::i2c0(
            ctx.device.I2C0,
            sda_pin,
            scl_pin,
            400.kHz(),
            &mut resets,
            &clocks.system_clock,
        );

        // Read runtime state from EEPROM (data address 0x50)
        let mut restored_state = midi_controller::state::PresetStateStore::new();
        let mut restored_active: u8 = 0;
        {
            use embedded_hal::i2c::I2c;
            let mut buf = [0u8; 128];
            if i2c.write_read(0x50u8, &[0x00u8], &mut buf).is_ok() {
                if let Some(store) = midi_controller::state::PresetStateStore::from_eeprom(&buf) {
                    restored_active = store.active_index();
                    restored_state = store;
                    info!("EEPROM: restored runtime state, preset {}", restored_active);
                }
            }
        }

        let i2c_bus = ctx.local.i2c_bus.write(AtomicCell::new(i2c));
        let mut displays = crate::hmi::display::Displays::new(
            AtomicDevice::new(i2c_bus),
            AtomicDevice::new(i2c_bus),
        );
        displays.splash_screen();
        let (usb_sender, usb_receiver) = make_channel!(UsbMidiEventPacket, USB_OUT_CAPACITY);
        send_to_usb_midi::spawn(usb_receiver).unwrap();

        let (display_sender, display_receiver) = make_channel!([u8; 3], DISPLAY_LOG_CAPACITY);
        let (display_event_sender, display_event_receiver) =
            make_channel!(pedalboard_midi::pe_handler::DisplayEvent, 4);

        let (led_sender, led_receiver) = make_channel!(LedEvent, LED_CAPACITY);
        let (din_thru_sender, din_thru_receiver) = make_channel!([u8; 3], DIN_THRU_CAPACITY);
        let (trigger_sender, trigger_receiver) = make_channel!([u8; 3], TRIGGER_CAPACITY);
        let (persist_sender, persist_receiver) =
            make_channel!(pedalboard_midi::persist::PersistCommand, PERSIST_CAPACITY);
        let (system_status_sender, system_status_receiver) =
            make_channel!(SystemStatus, SYSTEM_STATUS_CAPACITY);
        let (config_display_sender, config_display_receiver) = make_channel!(
            pedalboard_midi::config_mode::ConfigDisplayEvent,
            CONFIG_DISPLAY_CAPACITY
        );

        blink::spawn().unwrap();
        led_out::spawn(led_receiver).unwrap();
        poll_input::spawn(
            usb_sender.clone(),
            display_sender,
            display_event_sender,
            led_sender.clone(),
            persist_sender.clone(),
            config_display_sender,
        )
        .unwrap();
        display_out::spawn(
            display_receiver,
            display_event_receiver,
            system_status_receiver,
            config_display_receiver,
        )
        .unwrap();
        persist::spawn(persist_receiver, system_status_sender).unwrap();
        midi_clock::spawn(
            usb_sender.clone(),
            din_thru_sender.clone(),
            led_sender.clone(),
        )
        .unwrap();

        info!("pedalboard-midi {} initialized", env!("GIT_HASH"));

        // Presets loaded asynchronously in persist task
        let pe_config = midi_controller::config::Config::default();

        (
            Shared {
                usb_midi,
                usb_dev,
                active_preset: restored_active,
                pe_config,
                global_config: midi_controller::config::GlobalConfig::default(),
                state_store: restored_state,
                presets_skipped: 0,
                button_active: [false; 6],
            },
            Local {
                uart_midi_out,
                uart_midi_in,
                inputs,
                led_spi,
                displays,
                debug_led,
                led_sender_usb: led_sender,
                usb_sender_usb_thru: usb_sender.clone(),
                din_thru_receiver,
                din_thru_sender,
                trigger_sender_din: trigger_sender.clone(),
                trigger_sender_usb: trigger_sender.clone(),
                trigger_receiver,
                persist_sender,
                eeprom_i2c: AtomicDevice::new(i2c_bus),
            },
        )
    }

    #[task(binds = UART0_IRQ, local = [uart_midi_in, trigger_sender_din], shared = [])]
    fn midi_in(ctx: midi_in::Context) {
        match ctx.local.uart_midi_in.read() {
            Ok(m) => {
                let mut buf = [0x00u8; 3];
                m.render_slice(&mut buf);
                // All routing, reactive LEDs, and Mon LED handled in poll_input
                ctx.local.trigger_sender_din.try_send(buf).ok();
            }
            Err(nb::Error::WouldBlock) => {}
            Err(_) => error!("failed to receive midi message"),
        }
    }

    #[task(priority = 2, local = [inputs, uart_midi_out, din_thru_receiver, trigger_receiver], shared = [active_preset, pe_config, global_config, state_store, button_active])]
    async fn poll_input(
        mut ctx: poll_input::Context,
        mut sender: Sender<'static, UsbMidiEventPacket, USB_OUT_CAPACITY>,
        mut _display_sender: Sender<'static, [u8; 3], DISPLAY_LOG_CAPACITY>,
        mut display_event_sender: Sender<'static, pedalboard_midi::pe_handler::DisplayEvent, 4>,
        mut led_sender: Sender<'static, LedEvent, LED_CAPACITY>,
        mut persist_sender: Sender<
            'static,
            pedalboard_midi::persist::PersistCommand,
            PERSIST_CAPACITY,
        >,
        mut config_display_sender: Sender<
            'static,
            pedalboard_midi::config_mode::ConfigDisplayEvent,
            CONFIG_DISPLAY_CAPACITY,
        >,
    ) {
        let inputs = ctx.local.inputs;
        let uart_midi_out = ctx.local.uart_midi_out;
        let din_thru_receiver = ctx.local.din_thru_receiver;

        // Skip boot glitches — discard input events for first 200ms
        for _ in 0..200 {
            let mut discard = heapless::Vec::<_, 14>::new();
            inputs.poll_encoders(&mut discard);
            inputs.update();
            Mono::delay(1.millis()).await;
        }
        let mut pe = {
            let store = ctx.shared.state_store.lock(|s| s.clone());
            pedalboard_midi::pe_handler::PeHandler::with_state(store)
        };

        // Initial LED render from restored state
        {
            let preset_idx = ctx.shared.active_preset.lock(|p| *p);
            ctx.shared.pe_config.lock(|cfg| {
                if let Some(preset) = cfg.presets.get(preset_idx as usize) {
                    if !preset.name.is_empty() {
                        // Initialize Controller to the restored preset
                        let boot_result = pe.switch_to(preset_idx, cfg);
                        let anims = pe.led_state(preset);
                        led_sender.try_send(LedEvent::SetAllRings(anims)).ok();
                        // Send any MIDI from boot switch (on_enter actions)
                        for step in &boot_result.midi {
                            use midi_controller::routing::MidiPort;
                            use pedalboard_midi::pe_handler::MidiStep;
                            match step {
                                MidiStep::Send(raw, len, dest) => {
                                    if dest.contains(MidiPort::USB) {
                                        let packet = UsbMidiEventPacket::try_from_payload_bytes(
                                            CableNumber::Cable0,
                                            &raw[..*len],
                                        );
                                        if let Ok(packet) = packet {
                                            sender.try_send(packet).ok();
                                        }
                                    }
                                }
                                MidiStep::Delay(_) | MidiStep::SetLed { .. } => {}
                            }
                        }
                    }
                }
            });
            led_sender
                .try_send(LedEvent::SetSingle(
                    Led::Mode,
                    Some(pedalboard_midi::leds::preset_color(preset_idx)),
                ))
                .ok();
        }

        let mut bpm_displayed = false;
        let mut config_mode = pedalboard_midi::config_mode::ConfigMode::new();

        loop {
            // Drain USB→DIN thru messages
            while let Ok(raw) = din_thru_receiver.try_recv() {
                if let Ok(mm) = MidiMessage::try_parse_slice(&raw) {
                    uart_midi_out.write(&mm).ok();
                }
            }

            // Process incoming MIDI (routing, reactive LEDs, triggers)
            while let Ok(raw) = ctx.local.trigger_receiver.try_recv() {
                // Log incoming MIDI to config mode display.
                if config_mode.is_active() {
                    let msg_len: u8 = match raw[0] & 0xF0 {
                        0xC0 | 0xD0 => 2, // Program Change, Channel Pressure
                        _ => 3,
                    };
                    config_display_sender
                        .try_send(pedalboard_midi::config_mode::ConfigDisplayEvent::MidiIn {
                            data: raw,
                            len: msg_len,
                        })
                        .ok();
                }
                let result = ctx
                    .shared
                    .pe_config
                    .lock(|cfg| pe.process_incoming_midi(cfg, &raw));
                // Mon LED: blue flash for incoming MIDI activity
                led_sender
                    .try_send(LedEvent::Flash(
                        Led::Mon,
                        smart_leds::RGB8::new(0, 0, 64),
                        5,
                    ))
                    .ok();
                // Thru routing: send routed MIDI via DIN and/or USB
                for routed in &result.routed {
                    use midi_controller::routing::MidiPort;
                    let bytes = routed.bytes();
                    if routed.dest.contains(MidiPort::DIN) {
                        if let Ok(mm) = MidiMessage::try_parse_slice(bytes) {
                            uart_midi_out.write(&mm).ok();
                        }
                    }
                    if routed.dest.contains(MidiPort::USB) {
                        if let Ok(packet) =
                            UsbMidiEventPacket::try_from_payload_bytes(CableNumber::Cable0, bytes)
                        {
                            sender.try_send(packet).ok();
                        }
                    }
                }
                // Reactive LED from incoming CC
                if let Some(reactive) = &result.reactive_led {
                    use midi_controller::engine::ReactiveResult;
                    let evt = match reactive {
                        ReactiveResult::Heatmap(idx, fill) => {
                            LedEvent::SetReactiveRing(*idx, *fill)
                        }
                        ReactiveResult::Trigger(idx, active) => {
                            let anim = if *active {
                                let preset_idx = pe.active_preset() as usize;
                                ctx.shared.pe_config.lock(|cfg| {
                                    cfg.presets.get(preset_idx).map(|preset| {
                                        pedalboard_midi::pe_handler::button_ring_animation(
                                            preset, *idx,
                                        )
                                    })
                                })
                            } else {
                                None
                            };
                            LedEvent::SetReactiveTrigger(*idx, anim)
                        }
                    };
                    led_sender.try_send(evt).ok();
                }
                // Trigger-generated MIDI output
                for step in &result.midi {
                    use midi_controller::routing::MidiPort;
                    use pedalboard_midi::pe_handler::MidiStep;
                    match step {
                        MidiStep::Send(raw, len, dest) => {
                            let din_on = ctx.shared.global_config.lock(|gc| gc.din_enabled);
                            if din_on && dest.contains(MidiPort::DIN) {
                                if let Ok(mm) = MidiMessage::try_parse_slice(&raw[..*len]) {
                                    uart_midi_out.write(&mm).ok();
                                }
                            }
                            if dest.contains(MidiPort::USB) {
                                let packet = UsbMidiEventPacket::try_from_payload_bytes(
                                    CableNumber::Cable0,
                                    &raw[..*len],
                                );
                                if let Ok(packet) = packet {
                                    sender.try_send(packet).ok();
                                }
                            }
                        }
                        MidiStep::Delay(_) => {}
                        MidiStep::SetLed { .. } => {}
                    }
                }
                // Handle tap tempo from result
                if let Some(bpm) = result.bpm {
                    ctx.shared.global_config.lock(|gc| gc.bpm = bpm);
                }
                // Handle clock start/stop from button actions
                if let Some(running) = result.clock_running {
                    ctx.shared.global_config.lock(|gc| gc.midi_clock = running);
                }
                // Handle preset change
                if result.preset_changed {
                    let new_idx = pe.active_preset();
                    ctx.shared.active_preset.lock(|p| *p = new_idx);
                }
                if result.leds_changed || result.preset_changed {
                    let new_idx = pe.active_preset();
                    ctx.shared.pe_config.lock(|cfg| {
                        if let Some(preset) = cfg.presets.get(new_idx as usize) {
                            let anims = pe.led_state(preset);
                            led_sender.try_send(LedEvent::SetAllRings(anims)).ok();
                        }
                    });
                    ctx.shared.button_active.lock(|ba| *ba = pe.button_active());
                    if result.preset_changed {
                        let new_idx = pe.active_preset();
                        led_sender
                            .try_send(LedEvent::SetSingle(
                                Led::Mode,
                                Some(pedalboard_midi::leds::preset_color(new_idx)),
                            ))
                            .ok();
                    }
                }
            }

            let mut events = heapless::Vec::<_, 14>::new();
            inputs.poll_encoders(&mut events);
            let slow_events = inputs.update();
            for e in slow_events.iter() {
                events.push(*e).ok();
            }

            // Config mode: always process for entry/exit detection + diagnostics.
            let now_ms_cfg = (Mono::now().ticks() / 1_000) as u32;
            let config_active = {
                // Process when: config mode active, encoder buttons in events, or waiting for hold timeout.
                let has_encoder_buttons = events.iter().any(|e| {
                    matches!(
                        e,
                        pedalboard_midi::events::InputEvent::VolButton(_)
                            | pedalboard_midi::events::InputEvent::GainButton(_)
                    )
                });
                let need_full_processing =
                    config_mode.is_active() || has_encoder_buttons || config_mode.is_holding();

                if need_full_processing {
                    let preset_count = ctx.shared.pe_config.lock(|cfg| cfg.presets.len() as u8);
                    let (
                        din_enabled_cfg,
                        midi_clock_cfg,
                        bpm_cfg,
                        din_to_usb_cfg,
                        usb_to_din_cfg,
                        usb_to_usb_cfg,
                    ) = ctx.shared.global_config.lock(|gc| {
                        (
                            gc.din_enabled,
                            gc.midi_clock,
                            gc.bpm,
                            gc.din_to_usb_thru,
                            gc.usb_to_din_thru,
                            gc.usb_to_usb_thru,
                        )
                    });
                    let pidx = ctx.shared.active_preset.lock(|p| *p) as usize;

                    let (button_actions, encoder_configs, analog_configs) =
                        ctx.shared.pe_config.lock(|cfg| {
                            let mut actions = [
                                pedalboard_midi::config_mode::ButtonAction::default(),
                                pedalboard_midi::config_mode::ButtonAction::default(),
                                pedalboard_midi::config_mode::ButtonAction::default(),
                                pedalboard_midi::config_mode::ButtonAction::default(),
                                pedalboard_midi::config_mode::ButtonAction::default(),
                                pedalboard_midi::config_mode::ButtonAction::default(),
                            ];
                            let mut encs = [
                                pedalboard_midi::config_mode::EncoderInfo::default(),
                                pedalboard_midi::config_mode::EncoderInfo::default(),
                            ];
                            let mut analogs = [
                                pedalboard_midi::config_mode::AnalogInfo::default(),
                                pedalboard_midi::config_mode::AnalogInfo::default(),
                            ];
                            if let Some(preset) = cfg.presets.get(pidx) {
                                for (i, action) in actions.iter_mut().enumerate() {
                                    *action =
                                        pedalboard_midi::config_mode::summarize_button(preset, i);
                                }
                                for (i, enc) in encs.iter_mut().enumerate() {
                                    *enc =
                                        pedalboard_midi::config_mode::summarize_encoder(preset, i);
                                }
                                for (i, analog) in analogs.iter_mut().enumerate() {
                                    *analog =
                                        pedalboard_midi::config_mode::summarize_analog(preset, i);
                                }
                            }
                            (actions, encs, analogs)
                        });

                    let context = pedalboard_midi::config_mode::ConfigContext {
                        firmware_version: env!("CARGO_PKG_VERSION"),
                        git_hash: env!("GIT_HASH"),
                        preset_count,
                        din_enabled: din_enabled_cfg,
                        midi_clock: midi_clock_cfg,
                        bpm: bpm_cfg,
                        din_to_usb_thru: din_to_usb_cfg,
                        usb_to_din_thru: usb_to_din_cfg,
                        usb_to_usb_thru: usb_to_usb_cfg,
                        button_actions: &button_actions,
                        encoder_configs,
                        analog_configs,
                    };

                    let config_events = config_mode.process_events(&events, now_ms_cfg, &context);
                    for evt in config_events {
                        config_display_sender.try_send(evt).ok();
                    }
                }
                config_mode.is_active()
            };

            // When config mode is active, suppress normal display events but keep MIDI flowing.
            if config_active {
                // Drain display events from PE handler (don't show them on display).
                // MIDI processing continues below.
            }

            let mut pe_midi_steps: heapless::Vec<pedalboard_midi::pe_handler::MidiStep, 24> =
                heapless::Vec::new();
            let mut led_event: Option<LedEvent> = None;
            let mut din_enabled = true;

            let mut preset_idx = ctx.shared.active_preset.lock(|p| *p);

            // Process events through PE handler
            let need_tick = !events.is_empty() || pe.any_active();
            if need_tick {
                let now_ms = (Mono::now().ticks() / 1_000) as u32;
                let result = ctx
                    .shared
                    .pe_config
                    .lock(|cfg| pe.handle_events(cfg, &events, now_ms));
                for step in &result.midi {
                    pe_midi_steps.push(step.clone()).ok();
                }
                // Handle tap tempo from result
                if let Some(bpm) = result.bpm {
                    ctx.shared.global_config.lock(|gc| gc.bpm = bpm);
                    if !bpm_displayed && !config_active {
                        bpm_displayed = true;
                        display_event_sender
                            .try_send(pedalboard_midi::pe_handler::DisplayEvent::BpmOverlay { bpm })
                            .ok();
                    }
                }
                // Handle clock start/stop from button actions
                if let Some(running) = result.clock_running {
                    ctx.shared.global_config.lock(|gc| gc.midi_clock = running);
                }
                let new_preset = pe.active_preset();
                if result.preset_changed {
                    bpm_displayed = false;
                    ctx.shared.active_preset.lock(|p| *p = new_preset);
                }
                // Send display events directly (no MIDI round-trip)
                if !config_active {
                    for evt in result.display {
                        display_event_sender.try_send(evt).ok();
                    }
                }
                // Update LEDs and preset index on actual switch
                let led_dirty = result.leds_changed || result.preset_changed;
                if result.preset_changed {
                    preset_idx = new_preset;
                    led_sender
                        .try_send(LedEvent::SetSingle(
                            Led::Mode,
                            Some(pedalboard_midi::leds::preset_color(preset_idx)),
                        ))
                        .ok();
                }
                if !pe_midi_steps.is_empty() || led_dirty {
                    // Read DIN enabled from global config
                    din_enabled = ctx.shared.global_config.lock(|gc| gc.din_enabled);
                    if led_dirty {
                        ctx.shared.pe_config.lock(|cfg| {
                            let preset = &cfg.presets[preset_idx as usize];
                            let anims = pe.led_state(preset);
                            led_event = Some(LedEvent::SetAllRings(anims));
                        });
                        ctx.shared.button_active.lock(|ba| *ba = pe.button_active());
                    }
                }
                // Persist state changes to EEPROM/flash
                if result.preset_changed || led_dirty {
                    use pedalboard_midi::persist::PersistCommand;
                    if result.preset_changed {
                        persist_sender
                            .try_send(PersistCommand::SaveActivePreset(new_preset))
                            .ok();
                    }
                    persist_sender
                        .try_send(PersistCommand::SaveState(pe.eeprom_state()))
                        .ok();
                }
            }

            // Send MIDI outside the lock (latency-critical path first)
            let mut midi_sent = false;
            {
                use midi_controller::routing::MidiPort;
                use pedalboard_midi::pe_handler::MidiStep;
                for step in &pe_midi_steps {
                    match step {
                        MidiStep::Send(raw, len, dest) => {
                            midi_sent = true;
                            // Log outgoing MIDI to config mode display.
                            if config_active {
                                config_display_sender
                                    .try_send(
                                        pedalboard_midi::config_mode::ConfigDisplayEvent::MidiOut {
                                            data: *raw,
                                            len: *len as u8,
                                        },
                                    )
                                    .ok();
                            }
                            if din_enabled && dest.contains(MidiPort::DIN) {
                                if let Ok(mm) = MidiMessage::try_parse_slice(&raw[..*len]) {
                                    uart_midi_out.write(&mm).ok();
                                }
                            }
                            if dest.contains(MidiPort::USB) {
                                let packet = UsbMidiEventPacket::try_from_payload_bytes(
                                    CableNumber::Cable0,
                                    &raw[..*len],
                                );
                                if let Ok(packet) = packet {
                                    sender.try_send(packet).ok();
                                }
                            }
                            // Reactive LED: locally-generated CC also triggers reactive rings
                            if *len >= 3 && (raw[0] & 0xF0) == 0xB0 {
                                let channel = (raw[0] & 0x0F) + 1;
                                ctx.shared.pe_config.lock(|cfg| {
                                    if let Some(preset) = cfg.presets.get(preset_idx as usize) {
                                        if let Some(evt) =
                                            pedalboard_midi::pe_handler::reactive_led_event(
                                                preset, channel, raw[1], raw[2],
                                            )
                                        {
                                            led_sender.try_send(evt).ok();
                                        }
                                    }
                                });
                            }
                        }
                        MidiStep::Delay(ms) => {
                            Mono::delay(fugit::ExtU32::millis(*ms as u32).into()).await;
                        }
                        MidiStep::SetLed {
                            btn_idx,
                            color,
                            animation,
                        } => {
                            use midi_controller::config::LedAnimation;
                            use pedalboard_midi::ledring::{Modifier, Renderer, RingAnimation};
                            use pedalboard_midi::leds::{LedEvent, LedRings};
                            use pedalboard_midi::pe_handler::color_to_rgb;

                            let on_color = color_to_rgb(color);
                            let rgb = pedalboard_midi::ledring::rgb8_to_rgb(on_color);
                            let modifier = match animation {
                                LedAnimation::Solid => Modifier::Solid,
                                LedAnimation::Blink => Modifier::Blink,
                                LedAnimation::Pulse => Modifier::Pulse,
                                LedAnimation::Rotate => Modifier::Rotate,
                                LedAnimation::ColorCycle => Modifier::ColorCycle,
                            };
                            let ring_anim = RingAnimation {
                                renderer: Renderer::Solid(rgb),
                                modifier,
                            };
                            let ring = match btn_idx {
                                0 => LedRings::A,
                                1 => LedRings::B,
                                2 => LedRings::C,
                                3 => LedRings::D,
                                4 => LedRings::E,
                                _ => LedRings::F,
                            };
                            led_sender.try_send(LedEvent::SetRing(ring, ring_anim)).ok();
                        }
                    }
                }
            }
            // Flash Mon LED for outgoing MIDI activity
            if midi_sent {
                led_sender
                    .try_send(LedEvent::Flash(
                        Led::Mon,
                        smart_leds::RGB8::new(0, 64, 0),
                        5,
                    ))
                    .ok();
            }
            // Send LED update (visual feedback, not latency-critical)
            if let Some(evt) = led_event {
                led_sender.try_send(evt).ok();
            }
            Mono::delay(1.millis()).await;
        }
    }

    #[task(binds = USBCTRL_IRQ, priority = 3,
        local = [ buf: Vec::<u8, 350>=Vec::new(), led_sender_usb, usb_sender_usb_thru, din_thru_sender, trigger_sender_usb, persist_sender],
        shared =[usb_midi,usb_dev,pe_config,global_config,active_preset,presets_skipped]
    )]
    fn usb_rx(mut ctx: usb_rx::Context) {
        let sysex_receive_buffer = ctx.local.buf;

        let usb_dev = ctx.shared.usb_dev;
        let usb_midi = ctx.shared.usb_midi;

        let mut buffer = [0; 64];
        let mut received_size: usize = 0;

        (usb_dev, usb_midi).lock(|usb_dev, usb_midi| {
            if usb_dev.poll(&mut [usb_midi]) {
                match usb_midi.read(&mut buffer) {
                    Err(usb_device::UsbError::WouldBlock) => {}
                    Err(err) => {
                        error!("USB MIDI read error {:?}", err);
                    }
                    Ok(size) => {
                        received_size = size;
                    }
                }
            }
        });
        if received_size == 0 {
            return;
        }
        debug!("USB MIDI received");

        let buffer_reader = UsbMidiPacketReader::new(&buffer, received_size);
        for packet in buffer_reader.into_iter().flatten() {
            if !packet.is_sysex() {
                {
                    // All routing, reactive LEDs, and Mon LED handled in poll_input
                    let raw = packet.payload_bytes();
                    if raw.len() >= 3 {
                        let mut arr = [0u8; 3];
                        arr.copy_from_slice(&raw[..3]);
                        ctx.local.trigger_sender_usb.try_send(arr).ok();
                    }
                }
                continue;
            }

            if packet.is_sysex_start() {
                debug!("SysEx message start");
                sysex_receive_buffer.clear();
            }

            match sysex_receive_buffer.extend_from_slice(packet.payload_bytes()) {
                Ok(_) => {
                    if packet.is_sysex_end() {
                        debug!("SysEx IN  message: {:?}", sysex_receive_buffer);

                        // Handle MIDI-CI Property Exchange messages
                        if let Some(result) =
                            pedalboard_midi::pe_sysex::handle_set(sysex_receive_buffer.as_ref())
                        {
                            if let Some(cmd) = result.command {
                                ctx.local.persist_sender.try_send(cmd).ok();
                            }
                            for chunk in result.reply.chunks(3) {
                                if let Ok(p) = UsbMidiEventPacket::try_from_payload_bytes(
                                    CableNumber::Cable0,
                                    chunk,
                                ) {
                                    ctx.local.usb_sender_usb_thru.try_send(p).ok();
                                }
                            }
                            sysex_receive_buffer.clear();
                            continue;
                        }

                        // Handle Get Property Inquiry (read-back)
                        if midi_controller::property_exchange::is_get_property(
                            sysex_receive_buffer.as_ref(),
                        ) {
                            if let Some(resource) =
                                midi_controller::property_exchange::extract_get_resource(
                                    sysex_receive_buffer.as_ref(),
                                )
                            {
                                let req_id = midi_controller::property_exchange::request_id(
                                    sysex_receive_buffer.as_ref(),
                                );
                                let src_muid = midi_controller::property_exchange::source_muid(
                                    sysex_receive_buffer.as_ref(),
                                );
                                // Serialize from RAM for PE Get reply
                                static mut GET_BUF: [u8; pedalboard_midi::MAX_PRESET_SIZE] =
                                    [0u8; pedalboard_midi::MAX_PRESET_SIZE];
                                let body = if resource
                                    == midi_controller::config::GLOBAL_CONFIG_RESOURCE
                                {
                                    ctx.shared.global_config.lock(|gc| {
                                        let buf = unsafe { &mut *core::ptr::addr_of_mut!(GET_BUF) };
                                        postcard::to_slice(gc, buf).ok().map(|s| s.len())
                                    })
                                } else if resource == midi_controller::config::DEVICE_INFO_RESOURCE
                                {
                                    let mut version = heapless::String::<24>::new();
                                    let _ = core::fmt::Write::write_str(
                                        &mut version,
                                        concat!(env!("CARGO_PKG_VERSION"), "-", env!("GIT_HASH")),
                                    );
                                    let info = midi_controller::config::DeviceInfo {
                                        flash_format: pedalboard_midi::FLASH_FORMAT_VERSION,
                                        presets_loaded: ctx.shared.pe_config.lock(|cfg| {
                                            cfg.presets
                                                .iter()
                                                .filter(|p| !p.name.is_empty())
                                                .count()
                                                as u8
                                        }),
                                        presets_skipped: ctx.shared.presets_skipped.lock(|s| *s),
                                        version,
                                    };
                                    let buf = unsafe { &mut *core::ptr::addr_of_mut!(GET_BUF) };
                                    postcard::to_slice(&info, buf).ok().map(|s| s.len())
                                } else {
                                    ctx.shared.pe_config.lock(|cfg| {
                                        let buf = unsafe { &mut *core::ptr::addr_of_mut!(GET_BUF) };
                                        if let Some(preset) = cfg.presets.get(resource as usize) {
                                            if preset.name.is_empty() {
                                                None
                                            } else {
                                                postcard::to_slice(preset, buf)
                                                    .ok()
                                                    .map(|s| s.len())
                                            }
                                        } else {
                                            None
                                        }
                                    })
                                };
                                let reply_body: &[u8] = match body {
                                    Some(len) => unsafe {
                                        &(&(*core::ptr::addr_of!(GET_BUF)))[..len]
                                    },
                                    None => &[],
                                };
                                let get_status = if reply_body.is_empty() {
                                    midi_controller::property_exchange::PeStatus::NotFound
                                } else {
                                    midi_controller::property_exchange::PeStatus::Ok
                                };
                                let reply = midi_controller::property_exchange::build_get_reply(
                                    [0x01, 0x02, 0x03, 0x04],
                                    src_muid,
                                    req_id,
                                    resource,
                                    get_status,
                                    reply_body,
                                );
                                for chunk in reply.chunks(3) {
                                    if let Ok(p) = UsbMidiEventPacket::try_from_payload_bytes(
                                        CableNumber::Cable0,
                                        chunk,
                                    ) {
                                        ctx.local.usb_sender_usb_thru.try_send(p).ok();
                                    }
                                }
                            }
                            sysex_receive_buffer.clear();
                            continue;
                        }

                        // Unrecognized SysEx — ignored
                    }
                }
                Err(_) => {
                    error!("SysEx buffer overflow");
                    break;
                }
            }
        }
    }

    #[task(local = [eeprom_i2c], shared = [pe_config, global_config, active_preset, state_store, presets_skipped])]
    async fn persist(
        mut ctx: persist::Context,
        mut receiver: Receiver<'static, pedalboard_midi::persist::PersistCommand, PERSIST_CAPACITY>,
        mut status_sender: Sender<'static, SystemStatus, SYSTEM_STATUS_CAPACITY>,
    ) {
        let eeprom = ctx.local.eeprom_i2c;
        info!("config persistence: loading from flash");
        if let Some(mut store) = pedalboard_midi::storage::ConfigStore::try_new() {
            info!("config persistence ready");

            // Load presets from flash
            let mut preset_count = 0u8;
            store
                .load_all_presets(|idx, data| {
                    // Check flash format version prefix
                    if data.is_empty() {
                        return;
                    }
                    if data[0] != pedalboard_midi::FLASH_FORMAT_VERSION {
                        warn!(
                            "preset {}: flash format v{}, firmware expects v{} — skipped",
                            idx,
                            data[0],
                            pedalboard_midi::FLASH_FORMAT_VERSION
                        );
                        ctx.shared.presets_skipped.lock(|s| *s += 1);
                        return;
                    }
                    let payload = &data[1..]; // strip version byte
                    if let Ok(preset) =
                        postcard::from_bytes::<midi_controller::config::Preset>(payload)
                    {
                        ctx.shared.pe_config.lock(|cfg| {
                            let i = idx as usize;
                            while cfg.presets.len() <= i {
                                cfg.presets.push(Default::default()).ok();
                            }
                            cfg.presets[i] = preset;
                        });
                        preset_count += 1;
                    }
                })
                .await;
            if preset_count == 0 {
                info!("no presets loaded (empty or version mismatch)");
            } else {
                info!("{} presets loaded from flash", preset_count);
            }

            // Load global config from flash
            let mut gc_buf = [0u8; 64];
            if let Some(data) = store
                .load_preset(midi_controller::config::GLOBAL_CONFIG_RESOURCE, &mut gc_buf)
                .await
            {
                if data.is_empty() || data[0] != pedalboard_midi::FLASH_FORMAT_VERSION {
                    if !data.is_empty() {
                        warn!(
                            "global config: flash format v{}, firmware expects v{} — skipped",
                            data[0],
                            pedalboard_midi::FLASH_FORMAT_VERSION
                        );
                    }
                } else if let Ok(gc) =
                    postcard::from_bytes::<midi_controller::config::GlobalConfig>(&data[1..])
                {
                    info!("global config loaded from flash");
                    ctx.shared.global_config.lock(|g| *g = gc.clone());
                    ctx.shared.pe_config.lock(|cfg| cfg.global = gc);
                }
            }

            // Enter persist loop
            while let Ok(cmd) = receiver.recv().await {
                use pedalboard_midi::persist::PersistCommand;
                match cmd {
                    PersistCommand::SavePreset(preset_index, data) => {
                        // Prepend flash format version byte before storing
                        let mut versioned: heapless::Vec<
                            u8,
                            { pedalboard_midi::MAX_PRESET_SIZE + 1 },
                        > = heapless::Vec::new();
                        versioned.push(pedalboard_midi::FLASH_FORMAT_VERSION).ok();
                        versioned.extend_from_slice(&data).ok();

                        if data.is_empty() {
                            // Empty body = delete preset from flash and RAM
                            if preset_index == midi_controller::config::GLOBAL_CONFIG_RESOURCE {
                                info!("global config cleared");
                                ctx.shared.global_config.lock(|g| *g = Default::default());
                                ctx.shared
                                    .pe_config
                                    .lock(|cfg| cfg.global = Default::default());
                                let empty_marker: heapless::Vec<
                                    u8,
                                    { pedalboard_midi::MAX_PRESET_SIZE + 1 },
                                > = heapless::Vec::new();
                                store.save_preset(preset_index, &empty_marker).await;
                            } else {
                                // Only write to flash if the slot was actually occupied
                                let was_occupied = ctx.shared.pe_config.lock(|cfg| {
                                    let idx = preset_index as usize;
                                    if idx < cfg.presets.len() && !cfg.presets[idx].name.is_empty()
                                    {
                                        cfg.presets[idx] =
                                            midi_controller::config::Preset::default();
                                        true
                                    } else {
                                        false
                                    }
                                });
                                if was_occupied {
                                    info!("preset {} deleted", preset_index);
                                    let empty_marker: heapless::Vec<
                                        u8,
                                        { pedalboard_midi::MAX_PRESET_SIZE + 1 },
                                    > = heapless::Vec::new();
                                    store.save_preset(preset_index, &empty_marker).await;
                                }
                            }
                        } else if preset_index == midi_controller::config::GLOBAL_CONFIG_RESOURCE {
                            // Global config — apply and save to flash
                            if let Ok(gc) =
                                postcard::from_bytes::<midi_controller::config::GlobalConfig>(&data)
                            {
                                info!("global config applied and saved");
                                ctx.shared.global_config.lock(|g| *g = gc.clone());
                                ctx.shared.pe_config.lock(|cfg| cfg.global = gc);
                            }
                            store.save_preset(preset_index, &versioned).await;
                        } else if let Ok(preset) =
                            postcard::from_bytes::<midi_controller::config::Preset>(&data)
                        {
                            info!(
                                "preset {} loaded: \"{}\"",
                                preset_index,
                                preset.name.as_str()
                            );
                            ctx.shared.pe_config.lock(|cfg| {
                                let idx = preset_index as usize;
                                // Extend presets vec if needed
                                while cfg.presets.len() <= idx {
                                    cfg.presets
                                        .push(midi_controller::config::Preset::default())
                                        .ok();
                                }
                                cfg.presets[idx] = preset;
                            });
                            store.save_preset(preset_index, &versioned).await;
                            // Write initial state from preset defaults to EEPROM
                            let buf = ctx.shared.pe_config.lock(|cfg| {
                                let mut state_store =
                                    midi_controller::state::PresetStateStore::new();
                                for (i, p) in cfg.presets.iter().enumerate() {
                                    if i >= midi_controller::state::EEPROM_MAX_PRESETS {
                                        break;
                                    }
                                    if !p.defaults.button_active.is_empty()
                                        || !p.defaults.encoder_values.is_empty()
                                    {
                                        state_store.set_state(
                                            i,
                                            midi_controller::state::PresetState::from_defaults(p),
                                        );
                                    }
                                }
                                let mut buf = [0u8; 128];
                                state_store.to_eeprom(&mut buf);
                                buf
                            });
                            use embedded_hal::i2c::I2c;
                            for page in 0..16 {
                                let offset = page * 8;
                                let mut wbuf = [0u8; 9];
                                wbuf[0] = offset as u8;
                                wbuf[1..9].copy_from_slice(&buf[offset..offset + 8]);
                                eeprom.write(0x50u8, &wbuf).ok();
                                Mono::delay(5.millis()).await;
                            }
                        } else {
                            warn!("preset {} deserialize failed", preset_index);
                        }
                    }
                    PersistCommand::SaveActivePreset(idx) => {
                        store.save(8, 0, 0, idx as u16).await;
                    }
                    PersistCommand::SaveState(data) => {
                        // Write to AT24CS01 EEPROM at 0x50 in 8-byte pages
                        use embedded_hal::i2c::I2c;
                        for page in 0..16 {
                            let offset = page * 8;
                            let mut wbuf = [0u8; 9];
                            wbuf[0] = offset as u8;
                            wbuf[1..9].copy_from_slice(&data[offset..offset + 8]);
                            eeprom.write(0x50u8, &wbuf).ok();
                            Mono::delay(5.millis()).await;
                        }
                    }
                    PersistCommand::EraseAll => {
                        status_sender.try_send(SystemStatus::FactoryReset).ok();
                        Mono::delay(200.millis()).await;
                        store.erase_all().await;
                        // Clear EEPROM runtime state
                        let buf = midi_controller::state::DefaultPresetStateStore::cleared_eeprom();
                        use embedded_hal::i2c::I2c;
                        for page in 0..16 {
                            let offset = page * 8;
                            let mut wbuf = [0u8; 9];
                            wbuf[0] = offset as u8;
                            wbuf[1..9].copy_from_slice(&buf[offset..offset + 8]);
                            eeprom.write(0x50u8, &wbuf).ok();
                            Mono::delay(5.millis()).await;
                        }
                        info!("factory reset: storage + presets + eeprom erased, rebooting");
                        Mono::delay(1000.millis()).await;
                        cortex_m::peripheral::SCB::sys_reset();
                    }
                    PersistCommand::Reboot => {
                        status_sender.try_send(SystemStatus::Rebooting).ok();
                        Mono::delay(1000.millis()).await;
                        cortex_m::peripheral::SCB::sys_reset();
                    }
                    PersistCommand::Bootloader => {
                        status_sender.try_send(SystemStatus::Bootloader).ok();
                        Mono::delay(1000.millis()).await;
                        rp2040_hal::rom_data::reset_to_usb_boot(0, 0);
                    }
                }
            }
        } else {
            warn!("flash config store init failed, persistence disabled");
            while receiver.recv().await.is_ok() {}
        }
    }

    #[task(shared = [usb_midi, usb_dev])]
    async fn send_to_usb_midi(
        mut ctx: send_to_usb_midi::Context,
        mut receiver: Receiver<'static, UsbMidiEventPacket, USB_OUT_CAPACITY>,
    ) {
        // Wait until USB is configured
        loop {
            let configured = ctx
                .shared
                .usb_dev
                .lock(|usb_dev| usb_dev.state() == UsbDeviceState::Configured);
            if configured {
                break;
            }
            Mono::delay(100.millis()).await;
        }

        info!("USB MIDI out ready to send");
        let mut usb_midi = ctx.shared.usb_midi;
        while let Ok(packet) = receiver.recv().await {
            for _ in 0..10 {
                let result = usb_midi.lock(|usb_midi| usb_midi.send_packet(packet.clone()));
                match result {
                    Ok(_) => break,
                    Err(usb_device::UsbError::WouldBlock) => {
                        Mono::delay(1.millis()).await;
                    }
                    Err(_) => break,
                }
            }
        }
    }

    #[task(priority = 2, local = [led_spi, leds: pedalboard_midi::leds::Leds = pedalboard_midi::leds::Leds::new()])]
    async fn led_out(
        ctx: led_out::Context,
        mut receiver: Receiver<'static, LedEvent, LED_CAPACITY>,
    ) {
        let leds = ctx.local.leds;

        loop {
            // Drain all pending events
            while let Ok(evt) = receiver.try_recv() {
                leds.handle_event(evt);
            }

            // Tick animations
            leds.tick();

            // Render and write
            let data = leds.render();
            ctx.local
                .led_spi
                .write(brightness(data.iter().cloned(), 32))
                .unwrap();

            // 50Hz frame rate = 20ms
            Mono::delay(20.millis()).await;
        }
    }

    #[task(local = [displays], shared = [active_preset, pe_config, button_active])]
    async fn display_out(
        mut ctx: display_out::Context,
        mut receiver: Receiver<'static, [u8; 3], DISPLAY_LOG_CAPACITY>,
        mut event_receiver: Receiver<'static, pedalboard_midi::pe_handler::DisplayEvent, 4>,
        mut system_status_receiver: Receiver<'static, SystemStatus, SYSTEM_STATUS_CAPACITY>,
        mut config_display_receiver: Receiver<
            'static,
            pedalboard_midi::config_mode::ConfigDisplayEvent,
            CONFIG_DISPLAY_CAPACITY,
        >,
    ) {
        use crate::hmi::display::DisplayLocation;
        use heapless::String;
        use pedalboard_midi::views::performance::PresetMeta;

        let displays = ctx.local.displays;
        displays.splash_screen();
        Mono::delay(2000.millis()).await;

        // Load labels from PE config (defaults if empty)
        let mut presets: [PresetMeta; 32] = core::array::from_fn(|_| PresetMeta::default());
        ctx.shared.pe_config.lock(|cfg| {
            load_preset_meta(&mut presets, cfg);
        });

        let mut current_preset: u8 = 0;

        // If no presets have names, flash likely has stale/missing data — show hint
        let has_presets = presets.iter().any(|p| !p.name.is_empty());
        if !has_presets {
            displays.draw_message("No presets\n\nUpload\nsetlist");
            Mono::delay(3000.millis()).await;
        }

        displays.draw_performance(&presets[0]);

        // Overlay timeout: counts down each loop iteration (200ms each)
        let mut overlay_ticks: u8 = 0;
        const OVERLAY_DURATION: u8 = 5; // ~1s
        const PRESET_OVERLAY_DURATION: u8 = 10; // ~2s

        let mut midi_log = pedalboard_midi::display::MidiLog::new();
        let mut debug_mode = false;
        let mut debug_mode_ticks: u8 = 0;
        const DEBUG_MODE_TIMEOUT: u8 = 25; // 25 * 200ms = 5 seconds
        let mut config_mode_active = false;

        loop {
            let mut show_overlay = false;

            // System status takes priority — render and stop processing
            if let Ok(status) = system_status_receiver.try_recv() {
                displays.draw_system_status(status);
                // Hold display until reset occurs (persist task will reset after delay)
                loop {
                    Mono::delay(100.millis()).await;
                }
            }

            // Config mode display events.
            while let Ok(evt) = config_display_receiver.try_recv() {
                use pedalboard_midi::config_mode::ConfigDisplayEvent;
                match &evt {
                    ConfigDisplayEvent::Entered => {
                        config_mode_active = true;
                        displays.draw_config_entered();
                        midi_log = pedalboard_midi::display::MidiLog::new();
                    }
                    ConfigDisplayEvent::Info(info) => {
                        displays.draw_config_info(info);
                    }
                    ConfigDisplayEvent::ButtonPress { button, detail } => {
                        displays.draw_config_button_press(button, detail.as_str());
                    }
                    ConfigDisplayEvent::ButtonRelease { .. } => {
                        // Return to info screen after brief pause (handled by overlay timeout)
                    }
                    ConfigDisplayEvent::EncoderTurn {
                        encoder,
                        direction: _,
                        detail,
                    } => {
                        displays.draw_config_encoder_turn(encoder, detail.as_str());
                    }
                    ConfigDisplayEvent::ExpressionPedal {
                        pedal,
                        raw_adc,
                        detail,
                    } => {
                        displays.draw_config_expression(pedal, *raw_adc, detail.as_str());
                    }
                    ConfigDisplayEvent::MidiIn { data, len } => {
                        midi_log.push_midi('<', data, *len);
                        displays.draw_midi_log_right(&midi_log);
                    }
                    ConfigDisplayEvent::MidiOut { data, len } => {
                        midi_log.push_midi('>', data, *len);
                        displays.draw_midi_log_right(&midi_log);
                    }
                    ConfigDisplayEvent::Exited => {
                        config_mode_active = false;
                        // Restore normal performance view.
                        let idx = (current_preset as usize) % presets.len();
                        displays.draw_performance(&presets[idx]);
                    }
                }
            }

            // Skip normal display updates while in config mode.
            if config_mode_active {
                Mono::delay(200.millis()).await;
                continue;
            }

            // SysEx debug mode (currently unused)
            let sysex_active = false;
            if sysex_active && !debug_mode {
                debug_mode = true;
                debug_mode_ticks = DEBUG_MODE_TIMEOUT;
                displays.draw_midi_log(&midi_log);
            } else if sysex_active && debug_mode {
                debug_mode_ticks = DEBUG_MODE_TIMEOUT;
            } else if !sysex_active && debug_mode {
                debug_mode = false;
                let idx = (current_preset as usize) % presets.len();
                displays.draw_performance(&presets[idx]);
            }
            // Timeout: recover if ConnectionClose was never received
            if debug_mode && debug_mode_ticks > 0 {
                debug_mode_ticks -= 1;
                if debug_mode_ticks == 0 {
                    debug_mode = false;
                    let idx = (current_preset as usize) % presets.len();
                    displays.draw_performance(&presets[idx]);
                }
            }

            // Check for preset change
            let new_preset = ctx.shared.active_preset.lock(|p| *p);

            // Refresh labels from PE config
            let config_changed = ctx.shared.pe_config.lock(|cfg| {
                let mut changed = false;
                for (i, meta) in presets.iter_mut().enumerate() {
                    let (name, labels) =
                        pedalboard_midi::views::performance::preset_meta_from_config(cfg, i);
                    if meta.name != name || meta.button_labels != labels {
                        meta.name = name;
                        meta.button_labels = labels;
                        changed = true;
                    }
                }
                changed
            });

            // Refresh button active state
            let idx = (current_preset as usize) % presets.len();
            let active_changed = ctx.shared.button_active.lock(|ba| {
                if presets[idx].button_active != *ba {
                    presets[idx].button_active = *ba;
                    true
                } else {
                    false
                }
            });

            if (config_changed || active_changed) && !debug_mode {
                displays.draw_performance(&presets[idx]);
            }

            if new_preset != current_preset {
                let forward =
                    new_preset > current_preset || (current_preset > 0 && new_preset == 0);
                current_preset = new_preset;
                let idx = (current_preset as usize) % presets.len();
                if !debug_mode {
                    displays.draw_preset_overlay(
                        current_preset + 1,
                        presets[idx].name.as_str(),
                        forward,
                    );
                    overlay_ticks = PRESET_OVERLAY_DURATION;
                    show_overlay = true;
                }
            }

            // PE display events (direct from action layer, no MIDI round-trip)
            while let Ok(evt) = event_receiver.try_recv() {
                use crate::hmi::display::DisplayLocation;
                use pedalboard_midi::pe_handler::{DisplayEvent, DisplaySide, SystemAction};
                if !debug_mode {
                    match &evt {
                        DisplayEvent::EncoderOverlay { side, label, value }
                        | DisplayEvent::AnalogOverlay { side, label, value } => {
                            let loc = match side {
                                DisplaySide::L => DisplayLocation::L,
                                DisplaySide::R => DisplayLocation::R,
                            };
                            let lbl = if label.is_empty() {
                                match side {
                                    DisplaySide::L => "Vol",
                                    DisplaySide::R => "Gain",
                                }
                            } else {
                                label.as_str()
                            };
                            displays.draw_overlay(loc, lbl, *value);
                            overlay_ticks = OVERLAY_DURATION;
                            show_overlay = true;
                        }
                        DisplayEvent::LongPressHint { action } => {
                            let label = match action {
                                SystemAction::PresetNext => {
                                    let next = (current_preset as usize + 1) % presets.len();
                                    if presets[next].name.is_empty() {
                                        ">> Next"
                                    } else {
                                        presets[next].name.as_str()
                                    }
                                }
                                SystemAction::PresetPrev => {
                                    let prev = if current_preset == 0 {
                                        presets.len() - 1
                                    } else {
                                        (current_preset as usize) - 1
                                    };
                                    if presets[prev].name.is_empty() {
                                        "<< Prev"
                                    } else {
                                        presets[prev].name.as_str()
                                    }
                                }
                                SystemAction::PresetSelect(idx) => {
                                    let i = *idx as usize;
                                    if i < presets.len() && !presets[i].name.is_empty() {
                                        presets[i].name.as_str()
                                    } else {
                                        "Select"
                                    }
                                }
                                _ => action.label(),
                            };
                            displays.draw_long_press_hint(label);
                            show_overlay = true;
                        }
                        DisplayEvent::LongPressCancel => {
                            // Return to performance view
                            let idx = (current_preset as usize) % presets.len();
                            displays.draw_performance(&presets[idx]);
                            show_overlay = true;
                        }
                        DisplayEvent::BpmOverlay { bpm } => {
                            use core::fmt::Write;
                            let mut buf: heapless::String<16> = heapless::String::new();
                            if *bpm == 0 {
                                write!(buf, "Tap...").ok();
                            } else {
                                write!(buf, "BPM\n\n{}", bpm).ok();
                            }
                            displays.draw_message(buf.as_str());
                            overlay_ticks = OVERLAY_DURATION;
                            show_overlay = true;
                        }
                    }
                }
            }

            while let Ok(raw) = receiver.try_recv() {
                if debug_mode {
                    debug_mode_ticks = DEBUG_MODE_TIMEOUT;
                }
                let status = raw[0] & 0xF0;
                let ch = (raw[0] & 0x0F) + 1;
                match status {
                    0x90 => midi_log.push_note_on(ch, raw[1], raw[2]),
                    0x80 => midi_log.push_note_off(ch, raw[1]),
                    0xB0 => {
                        midi_log.push_cc(ch, raw[1], raw[2]);
                        if !debug_mode {
                            // Encoder/analog overlay: match CC# from PE config
                            let idx = (current_preset as usize) % presets.len();
                            ctx.shared.pe_config.lock(|cfg| {
                                let preset = cfg.presets.get(idx);
                                // Check encoder 0
                                if let Some(enc) = preset.and_then(|p| p.encoders.first()) {
                                    let enc_cc = match &enc.action {
                                        midi_controller::config::EncoderAction::Cc {
                                            cc, ..
                                        } => Some(*cc as u8),
                                        midi_controller::config::EncoderAction::CcRelative {
                                            cc,
                                            ..
                                        } => Some(*cc),
                                        _ => None,
                                    };
                                    if enc_cc == Some(raw[1]) {
                                        let lbl = if enc.label.is_empty() {
                                            String::try_from("Vol").unwrap_or_default()
                                        } else {
                                            enc.label.clone()
                                        };
                                        displays.draw_overlay(
                                            DisplayLocation::L,
                                            lbl.as_str(),
                                            raw[2],
                                        );
                                        overlay_ticks = OVERLAY_DURATION;
                                        show_overlay = true;
                                        return;
                                    }
                                }
                                // Check encoder 1
                                if let Some(enc) = preset.and_then(|p| p.encoders.get(1)) {
                                    let enc_cc = match &enc.action {
                                        midi_controller::config::EncoderAction::Cc {
                                            cc, ..
                                        } => Some(*cc as u8),
                                        midi_controller::config::EncoderAction::CcRelative {
                                            cc,
                                            ..
                                        } => Some(*cc),
                                        _ => None,
                                    };
                                    if enc_cc == Some(raw[1]) {
                                        let lbl = if enc.label.is_empty() {
                                            String::try_from("Gain").unwrap_or_default()
                                        } else {
                                            enc.label.clone()
                                        };
                                        displays.draw_overlay(
                                            DisplayLocation::R,
                                            lbl.as_str(),
                                            raw[2],
                                        );
                                        overlay_ticks = OVERLAY_DURATION;
                                        show_overlay = true;
                                        return;
                                    }
                                }
                                // Check analog 0
                                if let Some(a) = preset.and_then(|p| p.analog.first()) {
                                    if a.cc == raw[1] {
                                        let lbl = if a.label.is_empty() {
                                            String::try_from("Exp 1").unwrap_or_default()
                                        } else {
                                            a.label.clone()
                                        };
                                        displays.draw_overlay(
                                            DisplayLocation::L,
                                            lbl.as_str(),
                                            raw[2],
                                        );
                                        overlay_ticks = OVERLAY_DURATION;
                                        show_overlay = true;
                                        return;
                                    }
                                }
                                // Check analog 1
                                if let Some(a) = preset.and_then(|p| p.analog.get(1)) {
                                    if a.cc == raw[1] {
                                        let lbl = if a.label.is_empty() {
                                            String::try_from("Exp 2").unwrap_or_default()
                                        } else {
                                            a.label.clone()
                                        };
                                        displays.draw_overlay(
                                            DisplayLocation::R,
                                            lbl.as_str(),
                                            raw[2],
                                        );
                                        overlay_ticks = OVERLAY_DURATION;
                                        show_overlay = true;
                                        return;
                                    }
                                }
                                // Fallback: hardcoded CC#0/1/2/3 for default mode
                                match raw[1] {
                                    0 => {
                                        displays.draw_overlay(DisplayLocation::L, "Vol", raw[2]);
                                        overlay_ticks = OVERLAY_DURATION;
                                        show_overlay = true;
                                    }
                                    1 => {
                                        displays.draw_overlay(DisplayLocation::R, "Gain", raw[2]);
                                        overlay_ticks = OVERLAY_DURATION;
                                        show_overlay = true;
                                    }
                                    2 => {
                                        displays.draw_overlay(DisplayLocation::L, "Exp 1", raw[2]);
                                        overlay_ticks = OVERLAY_DURATION;
                                        show_overlay = true;
                                    }
                                    3 => {
                                        displays.draw_overlay(DisplayLocation::R, "Exp 2", raw[2]);
                                        overlay_ticks = OVERLAY_DURATION;
                                        show_overlay = true;
                                    }
                                    _ => {}
                                }
                            });
                        }
                    }
                    _ => {}
                }
            }

            if debug_mode {
                displays.draw_midi_log(&midi_log);
            } else if !show_overlay && overlay_ticks > 0 {
                overlay_ticks -= 1;
                if overlay_ticks == 0 {
                    let idx = (current_preset as usize) % presets.len();
                    displays.draw_performance(&presets[idx]);
                }
            }

            Mono::delay(200.millis()).await;
        }
    }

    #[task(priority = 2, shared = [global_config])]
    async fn midi_clock(
        mut ctx: midi_clock::Context,
        mut sender: Sender<'static, UsbMidiEventPacket, USB_OUT_CAPACITY>,
        mut din_sender: Sender<'static, [u8; 3], DIN_THRU_CAPACITY>,
        mut led_sender: Sender<'static, LedEvent, LED_CAPACITY>,
    ) {
        use midi_controller::clock::MidiClock;

        let mut clock = MidiClock::new();

        loop {
            let (clock_enabled, interval_us) = ctx.shared.global_config.lock(|gc| {
                let interval = if gc.midi_clock {
                    Some(gc.tick_interval_us())
                } else {
                    None
                };
                (gc.midi_clock, interval)
            });

            // Update clock state — sends Start/Stop on transitions.
            if let Some(output) = clock.update_config(clock_enabled) {
                dispatch_clock_messages(&output, &mut sender, &mut din_sender);
            }

            match interval_us {
                Some(us) => {
                    // Tick the clock — sends 0xF8 if running.
                    if let Some(output) = clock.tick() {
                        dispatch_clock_messages(&output, &mut sender, &mut din_sender);
                    }
                    // Sync LED animations to BPM
                    led_sender.try_send(LedEvent::BpmTick).ok();
                    Mono::delay((us as u64).micros()).await;
                }
                None => {
                    // Clock disabled, check again in 100ms
                    Mono::delay(100.millis()).await;
                }
            }
        }
    }

    /// Dispatch clock messages (F8/FA/FC/FB) to DIN and USB based on port flags.
    fn dispatch_clock_messages(
        output: &midi_controller::clock::ClockOutput,
        sender: &mut Sender<'static, UsbMidiEventPacket, USB_OUT_CAPACITY>,
        din_sender: &mut Sender<'static, [u8; 3], DIN_THRU_CAPACITY>,
    ) {
        use midi_controller::routing::MidiPort;

        for msg in &output.messages {
            let bytes = msg.bytes();
            if msg.dest.contains(MidiPort::DIN) {
                let mut raw = [0u8; 3];
                let len = bytes.len().min(3);
                raw[..len].copy_from_slice(&bytes[..len]);
                din_sender.try_send(raw).ok();
            }
            if msg.dest.contains(MidiPort::USB) {
                if let Ok(packet) =
                    UsbMidiEventPacket::try_from_payload_bytes(CableNumber::Cable0, bytes)
                {
                    sender.try_send(packet).ok();
                }
            }
        }
    }

    #[task(local = [debug_led, state: bool = false])]
    async fn blink(ctx: blink::Context) {
        loop {
            *ctx.local.state = !*ctx.local.state;
            if *ctx.local.state {
                ctx.local.debug_led.set_high().ok().unwrap();
            } else {
                ctx.local.debug_led.set_low().ok().unwrap();
            }
            Mono::delay(500.millis()).await;
        }
    }

    fn load_preset_meta(
        presets: &mut [pedalboard_midi::views::performance::PresetMeta; 32],
        cfg: &midi_controller::config::Config,
    ) {
        for (i, meta) in presets.iter_mut().enumerate() {
            let (name, labels) =
                pedalboard_midi::views::performance::preset_meta_from_config(cfg, i);
            meta.name = name;
            meta.button_labels = labels;
        }
    }
}

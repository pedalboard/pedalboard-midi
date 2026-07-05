#![no_std]
#![no_main]

mod hmi;

use defmt_rtt as _;
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
    #[cfg(feature = "opendeck")]
    use pedalboard_midi::opendeck_handler::{OpenDeck, OpenDeckConfigResponses};
    use pedalboard_midi::persist::PERSIST_CAPACITY;
    use pedalboard_midi::system_status::SystemStatus;
    #[cfg(feature = "opendeck")]
    use rtic_sync::channel::TrySendError;
    use rtic_sync::channel::{Receiver, Sender};
    use rtic_sync::make_channel;

    #[cfg(not(feature = "opendeck"))]
    pub struct OpenDeck;
    #[cfg(not(feature = "opendeck"))]
    pub enum OpenDeckConfigResponses {
        None,
    }

    use heapless::Vec;
    #[cfg(feature = "opendeck")]
    use midi2::Data;
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
        opendeck: OpenDeck,
        active_preset: u8,
        pe_config: pedalboard_protocol::config::Config,
        global_config: pedalboard_protocol::config::GlobalConfig,
        state_store: pedalboard_protocol::state::PresetStateStore,
        presets_skipped: u8,
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
        sender: Sender<'static, OpenDeckConfigResponses, SYSEX_CAPACITY>,
        led_sender_midi: Sender<'static, LedEvent, LED_CAPACITY>,
        led_sender_usb: Sender<'static, LedEvent, LED_CAPACITY>,
        usb_sender_din_thru: Sender<'static, UsbMidiEventPacket, USB_OUT_CAPACITY>,
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
    const SYSEX_CAPACITY: usize = 1;
    const DISPLAY_LOG_CAPACITY: usize = 8;
    const LED_CAPACITY: usize = 4;
    const DIN_THRU_CAPACITY: usize = 8;
    const TRIGGER_CAPACITY: usize = 8;
    const SYSTEM_STATUS_CAPACITY: usize = 1;

    #[init(local = [
        usb_bus: MaybeUninit<usb_device::bus::UsbBusAllocator<UsbBus>> = MaybeUninit::uninit(),
        i2c_bus: MaybeUninit<AtomicCell<I2CBus>> = MaybeUninit::uninit(),
        adc: MaybeUninit<rp2040_hal::adc::Adc> = MaybeUninit::uninit()
    ])]
    fn init(ctx: init::Context) -> (Shared, Local) {
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
                .product("pedalboard OpenDeck")
                .manufacturer("github.com/pedalboard")
                .serial_number("1.0.0")])
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
        let mut restored_state = pedalboard_protocol::state::PresetStateStore::new();
        let mut restored_active: u8 = 0;
        {
            use embedded_hal::i2c::I2c;
            let mut buf = [0u8; 128];
            if i2c.write_read(0x50u8, &[0x00u8], &mut buf).is_ok() {
                if let Some(store) = pedalboard_protocol::state::PresetStateStore::from_eeprom(&buf)
                {
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

        let (sysex_sender, sysex_receiver) = make_channel!(OpenDeckConfigResponses, SYSEX_CAPACITY);
        sysex_processor::spawn(sysex_receiver, usb_sender.clone()).unwrap();

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

        blink::spawn().unwrap();
        led_out::spawn(led_receiver).unwrap();
        poll_input::spawn(
            usb_sender.clone(),
            display_sender,
            display_event_sender,
            led_sender.clone(),
            persist_sender.clone(),
        )
        .unwrap();
        display_out::spawn(
            display_receiver,
            display_event_receiver,
            system_status_receiver,
        )
        .unwrap();
        persist::spawn(persist_receiver, system_status_sender).unwrap();
        midi_clock::spawn(
            usb_sender.clone(),
            din_thru_sender.clone(),
            led_sender.clone(),
        )
        .unwrap();

        #[cfg(feature = "opendeck")]
        let opendeck = OpenDeck::new(
            opendeck::config::FirmwareVersion {
                major: 1,
                minor: 0,
                revision: 0,
            },
            0x123456,
            persist_sender.clone(),
        );
        #[cfg(not(feature = "opendeck"))]
        let opendeck = OpenDeck;

        info!("pedalboard-midi {} initialized", env!("GIT_HASH"));

        // Presets loaded asynchronously in persist task
        let pe_config = pedalboard_protocol::config::Config::default();

        (
            Shared {
                usb_midi,
                usb_dev,
                opendeck,
                active_preset: restored_active,
                pe_config,
                global_config: pedalboard_protocol::config::GlobalConfig::default(),
                state_store: restored_state,
                presets_skipped: 0,
            },
            Local {
                uart_midi_out,
                uart_midi_in,
                inputs,
                led_spi,
                displays,
                debug_led,
                sender: sysex_sender,
                led_sender_midi: led_sender.clone(),
                led_sender_usb: led_sender,
                usb_sender_din_thru: usb_sender.clone(),
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

    #[task(binds = UART0_IRQ, local = [uart_midi_in, led_sender_midi, usb_sender_din_thru, trigger_sender_din], shared = [opendeck, global_config, pe_config, active_preset])]
    fn midi_in(mut ctx: midi_in::Context) {
        #[cfg(feature = "opendeck")]
        use midi2::prelude::*;

        match ctx.local.uart_midi_in.read() {
            Ok(m) => {
                let thru = ctx.shared.global_config.lock(|gc| gc.din_to_usb_thru);
                let mut buf = [0x00u8; 3];
                m.render_slice(&mut buf);
                #[cfg(feature = "opendeck")]
                ctx.shared.opendeck.lock(|opendeck| {
                    if let Ok(m) = BytesMessage::try_from(&buf[..]) {
                        opendeck.handle_midi_input(&m);
                    }
                });
                // Reactive LED: check incoming CC against active preset's listen_cc bindings
                if (buf[0] & 0xF0) == 0xB0 {
                    let channel = (buf[0] & 0x0F) + 1;
                    let cc = buf[1];
                    let value = buf[2];
                    let preset_idx = ctx.shared.active_preset.lock(|p| *p);
                    ctx.shared.pe_config.lock(|cfg| {
                        if let Some(preset) = cfg.presets.get(preset_idx as usize) {
                            if let Some(result) = pedalboard_protocol::engine::process_incoming_cc(
                                preset, channel, cc, value,
                            ) {
                                use pedalboard_midi::leds::LedEvent;
                                use pedalboard_protocol::engine::ReactiveResult;
                                let evt = match result {
                                    ReactiveResult::Heatmap(idx, fill) => {
                                        LedEvent::SetReactiveRing(idx, fill)
                                    }
                                    ReactiveResult::Trigger(idx, active) => {
                                        let anim = if active {
                                            Some(
                                                pedalboard_midi::pe_handler::button_ring_animation(
                                                    preset, idx,
                                                ),
                                            )
                                        } else {
                                            None
                                        };
                                        LedEvent::SetReactiveTrigger(idx, anim)
                                    }
                                };
                                ctx.local.led_sender_midi.try_send(evt).ok();
                            }
                        }
                    });
                }
                // Forward to trigger processor in poll_input
                ctx.local.trigger_sender_din.try_send(buf).ok();
                let din_to_usb = if thru { Some(buf) } else { None };
                // Flash Mon LED for MIDI activity (5 ticks = 100ms at 50Hz)
                ctx.local
                    .led_sender_midi
                    .try_send(LedEvent::Flash(
                        pedalboard_midi::leds::Led::Mon,
                        smart_leds::RGB8::new(0, 0, 64),
                        5,
                    ))
                    .ok();
                if let Some(raw) = din_to_usb {
                    let packet =
                        UsbMidiEventPacket::try_from_payload_bytes(CableNumber::Cable0, &raw);
                    if let Ok(packet) = packet {
                        ctx.local.usb_sender_din_thru.try_send(packet).ok();
                    }
                }
            }
            Err(nb::Error::WouldBlock) => {}
            Err(_) => error!("failed to receive midi message"),
        }
    }

    #[task(local = [inputs, uart_midi_out, din_thru_receiver, trigger_receiver], shared = [opendeck, active_preset, pe_config, global_config, state_store])]
    async fn poll_input(
        mut ctx: poll_input::Context,
        mut sender: Sender<'static, UsbMidiEventPacket, USB_OUT_CAPACITY>,
        mut display_sender: Sender<'static, [u8; 3], DISPLAY_LOG_CAPACITY>,
        mut display_event_sender: Sender<'static, pedalboard_midi::pe_handler::DisplayEvent, 4>,
        mut led_sender: Sender<'static, LedEvent, LED_CAPACITY>,
        mut persist_sender: Sender<
            'static,
            pedalboard_midi::persist::PersistCommand,
            PERSIST_CAPACITY,
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
        // Reset encoder values and rings after glitches
        #[cfg(feature = "opendeck")]
        ctx.shared.opendeck.lock(|opendeck| {
            use opendeck::encoder::EncoderSection;
            use opendeck::{Amount, Block, OpenDeckRequest, Wish};
            for i in 0..2u16 {
                opendeck.config.process_req(OpenDeckRequest::Configuration(
                    Wish::Set,
                    Amount::Single,
                    Block::Encoder(i, EncoderSection::RepeatedValue(0)),
                ));
            }
            opendeck.reset_encoder_rings();
        });

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
                        let anims = pe.led_state(preset);
                        led_sender.try_send(LedEvent::SetAllRings(anims)).ok();
                        // Fire on_enter for the boot preset
                        for action in &preset.on_enter {
                            match action {
                                pedalboard_protocol::config::Action::Delay(_) => {}
                                _ => {
                                    if let Some(msg) =
                                        pedalboard_protocol::action::action_to_midi(action)
                                    {
                                        let packet = UsbMidiEventPacket::try_from_payload_bytes(
                                            CableNumber::Cable0,
                                            &msg.data[..msg.len],
                                        );
                                        if let Ok(packet) = packet {
                                            sender.try_send(packet).ok();
                                        }
                                    }
                                }
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

        let mut tap_tempo = pedalboard_protocol::tap_tempo::TapTempo::new();
        let mut loop_tick_ms: u32 = 0;

        loop {
            // Tick encoder acceleration timer unconditionally
            pe.tick();
            loop_tick_ms = loop_tick_ms.wrapping_add(5); // ~5ms per loop iteration

            // Drain USB→DIN thru messages
            while let Ok(raw) = din_thru_receiver.try_recv() {
                if let Ok(mm) = MidiMessage::try_parse_slice(&raw) {
                    uart_midi_out.write(&mm).ok();
                }
            }

            // Process incoming MIDI triggers
            while let Ok(raw) = ctx.local.trigger_receiver.try_recv() {
                let trigger_preset_idx = ctx.shared.active_preset.lock(|p| *p);
                let trigger_result = ctx.shared.pe_config.lock(|cfg| {
                    if let Some(preset) = cfg.presets.get(trigger_preset_idx as usize) {
                        if !preset.triggers.is_empty() {
                            Some(pe.process_incoming_midi(preset, &raw))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });
                if let Some(result) = trigger_result {
                    for step in &result.midi {
                        use pedalboard_midi::pe_handler::MidiStep;
                        match step {
                            MidiStep::Send(raw, len) => {
                                let din_on = ctx.shared.global_config.lock(|gc| gc.din_enabled);
                                if din_on {
                                    if let Ok(mm) = MidiMessage::try_parse_slice(&raw[..*len]) {
                                        uart_midi_out.write(&mm).ok();
                                    }
                                }
                                let packet = UsbMidiEventPacket::try_from_payload_bytes(
                                    CableNumber::Cable0,
                                    &raw[..*len],
                                );
                                if let Ok(packet) = packet {
                                    sender.try_send(packet).ok();
                                }
                            }
                            MidiStep::Delay(_) => {}
                            MidiStep::SetLed { .. } => {}
                        }
                    }
                    for action in &result.system {
                        use pedalboard_protocol::engine::SystemAction;
                        ctx.shared.active_preset.lock(|active_preset| {
                            let active_count = ctx
                                .shared
                                .pe_config
                                .lock(|cfg| {
                                    cfg.presets.iter().filter(|p| !p.name.is_empty()).count() as u8
                                })
                                .max(1);
                            match action {
                                SystemAction::PresetNext => {
                                    *active_preset = (*active_preset + 1) % active_count;
                                }
                                SystemAction::PresetPrev => {
                                    *active_preset = if *active_preset == 0 {
                                        active_count - 1
                                    } else {
                                        *active_preset - 1
                                    };
                                }
                                SystemAction::PresetSelect(idx) => {
                                    if *idx < active_count {
                                        *active_preset = *idx;
                                    }
                                }
                                SystemAction::TapTempo | SystemAction::SetBpm(_) => {}
                            }
                        });
                    }
                    // Handle tap tempo (needs access to tap_tempo state outside the lock)
                    for action in &result.system {
                        if matches!(action, pedalboard_midi::pe_handler::SystemAction::TapTempo) {
                            if let Some(bpm) = tap_tempo.tap(loop_tick_ms) {
                                ctx.shared.global_config.lock(|gc| gc.bpm = bpm);
                            }
                        }
                    }
                    if result.led_dirty || !result.system.is_empty() {
                        let new_idx = ctx.shared.active_preset.lock(|p| *p);
                        ctx.shared.pe_config.lock(|cfg| {
                            if let Some(preset) = cfg.presets.get(new_idx as usize) {
                                let anims = pe.led_state(preset);
                                led_sender.try_send(LedEvent::SetAllRings(anims)).ok();
                            }
                        });
                        if !result.system.is_empty() {
                            let new_idx = ctx.shared.active_preset.lock(|p| *p);
                            led_sender
                                .try_send(LedEvent::SetSingle(
                                    Led::Mode,
                                    Some(pedalboard_midi::leds::preset_color(new_idx)),
                                ))
                                .ok();
                        }
                    }
                }
            }

            let mut events = heapless::Vec::<_, 14>::new();
            inputs.poll_encoders(&mut events);
            let slow_events = inputs.update();
            for e in slow_events.iter() {
                events.push(*e).ok();
            }

            #[allow(unused_mut)]
            let mut all_sent: heapless::Vec<([u8; 6], usize), 24> = heapless::Vec::new();
            let mut pe_midi_steps: heapless::Vec<pedalboard_midi::pe_handler::MidiStep, 24> =
                heapless::Vec::new();
            let mut led_event: Option<LedEvent> = None;
            let mut din_enabled = true;
            #[allow(unused_mut)]
            let mut component_info_buf: Option<([u8; 16], usize)> = None;

            // Determine if active preset is a PE preset (has a name)
            // Slots 0-3 can be either PE or OpenDeck; slots 4+ are PE-only
            let mut preset_idx = ctx.shared.active_preset.lock(|p| *p);
            let pe_active = ctx.shared.pe_config.lock(|cfg| {
                cfg.presets
                    .get(preset_idx as usize)
                    .map(|p| !p.name.is_empty())
                    .unwrap_or(false)
            });

            if pe_active {
                // PE mode: only lock pe_config when needed
                let need_tick = !events.is_empty() || pe.any_active();
                if need_tick {
                    let cal = ctx.shared.global_config.lock(|gc| {
                        pedalboard_midi::pe_handler::AdcCalibration {
                            exp1_min: gc.exp1_min,
                            exp1_max: gc.exp1_max,
                            exp2_min: gc.exp2_min,
                            exp2_max: gc.exp2_max,
                        }
                    });
                    let result = ctx.shared.pe_config.lock(|cfg| {
                        let preset = &cfg.presets[preset_idx as usize];
                        pe.handle_events(preset, &events, &cal)
                    });
                    for step in &result.midi {
                        pe_midi_steps.push(step.clone()).ok();
                    }
                    // Handle system actions (preset switching)
                    for action in &result.system {
                        use pedalboard_midi::pe_handler::SystemAction;
                        ctx.shared.active_preset.lock(|active_preset| {
                            let active_count = ctx
                                .shared
                                .pe_config
                                .lock(|cfg| {
                                    cfg.presets.iter().filter(|p| !p.name.is_empty()).count() as u8
                                })
                                .max(1);
                            match action {
                                SystemAction::PresetNext => {
                                    *active_preset = (*active_preset + 1) % active_count;
                                }
                                SystemAction::PresetPrev => {
                                    *active_preset = if *active_preset == 0 {
                                        active_count - 1
                                    } else {
                                        *active_preset - 1
                                    };
                                }
                                SystemAction::PresetSelect(idx) => {
                                    if *idx < active_count {
                                        *active_preset = *idx;
                                    }
                                }
                                SystemAction::TapTempo | SystemAction::SetBpm(_) => {}
                            }
                        });
                    }
                    if !result.system.is_empty() {
                        let new_preset = ctx.shared.active_preset.lock(|p| *p);
                        // Switch preset state and recall MIDI to external gear
                        let switch_midi = ctx.shared.pe_config.lock(|cfg| {
                            let old_preset = &cfg.presets[preset_idx as usize];
                            let new_preset_cfg = &cfg.presets[new_preset as usize];
                            pe.switch_preset(new_preset, old_preset, new_preset_cfg)
                        });
                        for step in &switch_midi {
                            pe_midi_steps.push(step.clone()).ok();
                        }
                        use pedalboard_midi::persist::PersistCommand;
                        persist_sender
                            .try_send(PersistCommand::SaveActivePreset(new_preset))
                            .ok();
                        persist_sender
                            .try_send(PersistCommand::SaveState(pe.eeprom_state()))
                            .ok();
                    }
                    // Send display events directly (no MIDI round-trip)
                    for evt in result.display {
                        display_event_sender.try_send(evt).ok();
                    }
                    // Preset switch always dirties LEDs (new colors/states)
                    let led_dirty = result.led_dirty || !result.system.is_empty();
                    if !result.system.is_empty() {
                        preset_idx = ctx.shared.active_preset.lock(|p| *p);
                        led_sender
                            .try_send(LedEvent::SetSingle(
                                Led::Mode,
                                Some(pedalboard_midi::leds::preset_color(preset_idx)),
                            ))
                            .ok();
                    }
                    if !pe_midi_steps.is_empty() || led_dirty {
                        // Read DIN enabled from global config (PE mode — no OpenDeck needed)
                        din_enabled = ctx.shared.global_config.lock(|gc| gc.din_enabled);
                        if led_dirty {
                            ctx.shared.pe_config.lock(|cfg| {
                                let preset = &cfg.presets[preset_idx as usize];
                                let anims = pe.led_state(preset);
                                led_event = Some(LedEvent::SetAllRings(anims));
                            });
                        }
                    }
                    // Persist state to EEPROM on any state change
                    if led_dirty && result.system.is_empty() {
                        use pedalboard_midi::persist::PersistCommand;
                        persist_sender
                            .try_send(PersistCommand::SaveState(pe.eeprom_state()))
                            .ok();
                    }
                }
            } else if !events.is_empty() && preset_idx < 4 {
                // OpenDeck mode (slots 0-3 only): sync preset and handle input
                #[cfg(feature = "opendeck")]
                {
                    din_enabled = ctx.shared.global_config.lock(|gc| gc.din_enabled);
                    ctx.shared.opendeck.lock(|opendeck| {
                        let (sent, anims, ci) = opendeck.process_events(preset_idx, &events);
                        all_sent = sent;
                        led_event = Some(LedEvent::SetAllRings(anims));
                        component_info_buf = ci;
                    });
                }
            }
            // Slots 4+ with no PE preset: inputs are silent
            // Send LED update outside the lock
            if let Some(evt) = led_event {
                led_sender.try_send(evt).ok();
            }
            // Send MIDI outside the lock
            let mut midi_sent = false;
            if pe_active {
                use pedalboard_midi::pe_handler::MidiStep;
                for step in &pe_midi_steps {
                    match step {
                        MidiStep::Send(raw, len) => {
                            midi_sent = true;
                            if din_enabled {
                                if let Ok(mm) = MidiMessage::try_parse_slice(&raw[..*len]) {
                                    uart_midi_out.write(&mm).ok();
                                }
                            }
                            let packet = UsbMidiEventPacket::try_from_payload_bytes(
                                CableNumber::Cable0,
                                &raw[..*len],
                            );
                            if let Ok(packet) = packet {
                                sender.try_send(packet).ok();
                            }
                            // Reactive LED: locally-generated CC also triggers reactive rings
                            if *len >= 3 && (raw[0] & 0xF0) == 0xB0 {
                                let channel = (raw[0] & 0x0F) + 1;
                                ctx.shared.pe_config.lock(|cfg| {
                                    if let Some(preset) = cfg.presets.get(preset_idx as usize) {
                                        if let Some(result) =
                                            pedalboard_protocol::engine::process_incoming_cc(
                                                preset, channel, raw[1], raw[2],
                                            )
                                        {
                                            use pedalboard_protocol::engine::ReactiveResult;
                                            let evt = match result {
                                                ReactiveResult::Heatmap(idx, fill) => {
                                                    LedEvent::SetReactiveRing(idx, fill)
                                                }
                                                ReactiveResult::Trigger(idx, active) => {
                                                    let anim = if active {
                                                        Some(pedalboard_midi::pe_handler::button_ring_animation(preset, idx))
                                                    } else {
                                                        None
                                                    };
                                                    LedEvent::SetReactiveTrigger(idx, anim)
                                                }
                                            };
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
                            use pedalboard_midi::ledring::{Modifier, Renderer, RingAnimation};
                            use pedalboard_midi::leds::{LedEvent, LedRings};
                            use pedalboard_midi::pe_handler::color_to_rgb;
                            use pedalboard_protocol::config::LedAnimation;

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
            for (raw, len) in &all_sent {
                midi_sent = true;
                // DIN MIDI out (only if enabled)
                if din_enabled {
                    if let Ok(mm) = MidiMessage::try_parse_slice(&raw[..*len]) {
                        uart_midi_out.write(&mm).ok();
                    }
                }
                // Display log (only in OpenDeck mode — PE overlay is handled separately)
                if !pe_active {
                    let mut display_raw = [0u8; 3];
                    let copy_len = (*len).min(3);
                    display_raw[..copy_len].copy_from_slice(&raw[..copy_len]);
                    display_sender.try_send(display_raw).ok();
                }
                // USB MIDI out (always for locally-generated messages)
                let packet =
                    UsbMidiEventPacket::try_from_payload_bytes(CableNumber::Cable0, &raw[..*len]);
                if let Ok(packet) = packet {
                    sender.try_send(packet).ok();
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
            // Send component info SysEx (chunked into 3-byte USB MIDI packets)
            if let Some((ci_buf, ci_len)) = component_info_buf {
                for chunk in ci_buf[..ci_len].chunks(3) {
                    if let Ok(packet) =
                        UsbMidiEventPacket::try_from_payload_bytes(CableNumber::Cable0, chunk)
                    {
                        sender.try_send(packet).ok();
                    }
                }
            }
            Mono::delay(1.millis()).await;
        }
    }

    #[task(binds = USBCTRL_IRQ, priority = 3,
        local = [ buf: Vec::<u8, 350>=Vec::new(), sender, led_sender_usb, usb_sender_usb_thru, din_thru_sender, trigger_sender_usb, persist_sender],
        shared =[usb_midi,usb_dev,opendeck,pe_config,global_config,active_preset,presets_skipped]
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
                #[allow(unused_variables)]
                if let Ok(m) = midi2::BytesMessage::try_from(packet.payload_bytes()) {
                    let (usb_to_din, usb_to_usb) = ctx
                        .shared
                        .global_config
                        .lock(|gc| (gc.usb_to_din_thru, gc.usb_to_usb_thru));
                    #[cfg(feature = "opendeck")]
                    ctx.shared.opendeck.lock(|opendeck| {
                        opendeck.handle_midi_input(&m);
                    });
                    // Reactive LED: check incoming CC against active preset's listen_cc bindings
                    let raw = packet.payload_bytes();
                    if raw.len() >= 3 && (raw[0] & 0xF0) == 0xB0 {
                        let channel = (raw[0] & 0x0F) + 1;
                        let cc = raw[1];
                        let value = raw[2];
                        let preset_idx = ctx.shared.active_preset.lock(|p| *p);
                        ctx.shared.pe_config.lock(|cfg| {
                            if let Some(preset) = cfg.presets.get(preset_idx as usize) {
                                if let Some(result) =
                                    pedalboard_protocol::engine::process_incoming_cc(
                                        preset, channel, cc, value,
                                    )
                                {
                                    use pedalboard_protocol::engine::ReactiveResult;
                                    let evt = match result {
                                        ReactiveResult::Heatmap(idx, fill) => {
                                            LedEvent::SetReactiveRing(idx, fill)
                                        }
                                        ReactiveResult::Trigger(idx, active) => {
                                            let anim = if active {
                                                Some(pedalboard_midi::pe_handler::button_ring_animation(preset, idx))
                                            } else {
                                                None
                                            };
                                            LedEvent::SetReactiveTrigger(idx, anim)
                                        }
                                    };
                                    ctx.local.led_sender_usb.try_send(evt).ok();
                                }
                            }
                        });
                    }
                    // Forward to trigger processor in poll_input
                    {
                        let raw = packet.payload_bytes();
                        if raw.len() >= 3 {
                            let mut arr = [0u8; 3];
                            arr.copy_from_slice(&raw[..3]);
                            ctx.local.trigger_sender_usb.try_send(arr).ok();
                        }
                    }
                    // Flash Mon LED for MIDI activity (5 ticks = 100ms at 50Hz)
                    ctx.local
                        .led_sender_usb
                        .try_send(LedEvent::Flash(
                            pedalboard_midi::leds::Led::Mon,
                            smart_leds::RGB8::new(0, 0, 64),
                            5,
                        ))
                        .ok();
                    // USB→DIN thru
                    if usb_to_din {
                        let raw = packet.payload_bytes();
                        if raw.len() >= 3 {
                            let mut arr = [0u8; 3];
                            arr.copy_from_slice(&raw[..3]);
                            ctx.local.din_thru_sender.try_send(arr).ok();
                        }
                    }
                    // USB→USB thru
                    if usb_to_usb {
                        ctx.local.usb_sender_usb_thru.try_send(packet.clone()).ok();
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
                        if pedalboard_protocol::property_exchange::is_set_property(
                            sysex_receive_buffer.as_ref(),
                        ) {
                            if let Some(data) =
                                pedalboard_protocol::property_exchange::extract_set_property(
                                    sysex_receive_buffer.as_ref(),
                                )
                            {
                                let mut decoded = [0u8; pedalboard_midi::MAX_PRESET_SIZE];
                                let dec_len =
                                    pedalboard_protocol::property_exchange::decode_mcoded7(
                                        data.body,
                                        &mut decoded,
                                    );

                                if data.resource
                                    == pedalboard_protocol::config::SYSTEM_COMMAND_RESOURCE
                                {
                                    // System command
                                    if dec_len > 0 {
                                        if let Some(cmd) =
                                            pedalboard_protocol::config::SystemCommand::from_byte(
                                                decoded[0],
                                            )
                                        {
                                            use pedalboard_midi::persist::PersistCommand;
                                            debug!("PE System command: {}", cmd as u8);
                                            let persist_cmd = match cmd {
                                                pedalboard_protocol::config::SystemCommand::Reboot => {
                                                    PersistCommand::Reboot
                                                }
                                                pedalboard_protocol::config::SystemCommand::Bootloader => {
                                                    PersistCommand::Bootloader
                                                }
                                                pedalboard_protocol::config::SystemCommand::FactoryReset => {
                                                    PersistCommand::EraseAll
                                                }
                                            };
                                            ctx.local.persist_sender.try_send(persist_cmd).ok();
                                        }
                                    }
                                } else if data.resource
                                    == pedalboard_protocol::config::GLOBAL_CONFIG_RESOURCE
                                {
                                    // Global config — save via persist task (applied on load)
                                    debug!("PE Set GlobalConfig body len={}", dec_len);
                                    if let Ok(blob) = heapless::Vec::from_slice(&decoded[..dec_len])
                                    {
                                        ctx.local
                                            .persist_sender
                                            .try_send(pedalboard_midi::persist::PersistCommand::SavePreset(
                                                pedalboard_protocol::config::GLOBAL_CONFIG_RESOURCE,
                                                blob,
                                            ))
                                            .ok();
                                    }
                                } else {
                                    // Preset
                                    debug!(
                                        "PE Set Property preset={} body len={}",
                                        data.resource,
                                        data.body.len()
                                    );
                                    if let Ok(blob) = heapless::Vec::from_slice(&decoded[..dec_len])
                                    {
                                        ctx.local
                                            .persist_sender
                                            .try_send(pedalboard_midi::persist::PersistCommand::SavePreset(
                                                data.resource,
                                                blob,
                                            ))
                                            .ok();
                                    }
                                }

                                // Send ACK reply
                                let req_id = pedalboard_protocol::property_exchange::request_id(
                                    sysex_receive_buffer.as_ref(),
                                );
                                let src_muid = pedalboard_protocol::property_exchange::source_muid(
                                    sysex_receive_buffer.as_ref(),
                                );
                                let reply = pedalboard_protocol::property_exchange::build_set_reply(
                                    [0x01, 0x02, 0x03, 0x04],
                                    src_muid,
                                    req_id,
                                    pedalboard_protocol::property_exchange::PeStatus::Ok,
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

                        // Handle Get Property Inquiry (read-back)
                        if pedalboard_protocol::property_exchange::is_get_property(
                            sysex_receive_buffer.as_ref(),
                        ) {
                            if let Some(resource) =
                                pedalboard_protocol::property_exchange::extract_get_resource(
                                    sysex_receive_buffer.as_ref(),
                                )
                            {
                                let req_id = pedalboard_protocol::property_exchange::request_id(
                                    sysex_receive_buffer.as_ref(),
                                );
                                let src_muid = pedalboard_protocol::property_exchange::source_muid(
                                    sysex_receive_buffer.as_ref(),
                                );
                                // Serialize from RAM for PE Get reply
                                static mut GET_BUF: [u8; pedalboard_midi::MAX_PRESET_SIZE] =
                                    [0u8; pedalboard_midi::MAX_PRESET_SIZE];
                                let body = if resource
                                    == pedalboard_protocol::config::GLOBAL_CONFIG_RESOURCE
                                {
                                    ctx.shared.global_config.lock(|gc| {
                                        let buf = unsafe { &mut *core::ptr::addr_of_mut!(GET_BUF) };
                                        postcard::to_slice(gc, buf).ok().map(|s| s.len())
                                    })
                                } else if resource
                                    == pedalboard_protocol::config::DEVICE_INFO_RESOURCE
                                {
                                    let info = pedalboard_protocol::config::DeviceInfo {
                                        flash_format: pedalboard_midi::FLASH_FORMAT_VERSION,
                                        presets_loaded: ctx.shared.pe_config.lock(|cfg| {
                                            cfg.presets
                                                .iter()
                                                .filter(|p| !p.name.is_empty())
                                                .count()
                                                as u8
                                        }),
                                        presets_skipped: ctx.shared.presets_skipped.lock(|s| *s),
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
                                    pedalboard_protocol::property_exchange::PeStatus::NotFound
                                } else {
                                    pedalboard_protocol::property_exchange::PeStatus::Ok
                                };
                                let reply = pedalboard_protocol::property_exchange::build_get_reply(
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

                        // OpenDeck SysEx handling continues below
                        #[cfg(feature = "opendeck")]
                        {
                            // Detect SET SINGLE commands and persist them
                            // Format: F0 00 53 43 00 PP 01 00 BLOCK SECTION IDX_H IDX_L VAL_H VAL_L F7
                            if sysex_receive_buffer.len() >= 15
                                && sysex_receive_buffer[6] == 0x01  // WISH = SET
                                && sysex_receive_buffer[7] == 0x00
                            // AMOUNT = SINGLE
                            {
                                let block = sysex_receive_buffer[8];
                                let section = sysex_receive_buffer[9];
                                let index = sysex_receive_buffer[11]; // LSB
                                let value = ((sysex_receive_buffer[12] as u16) << 8)
                                    | sysex_receive_buffer[13] as u16;
                                ctx.local
                                    .persist_sender
                                    .try_send(pedalboard_midi::persist::PersistCommand::Save(
                                        block, section, index, value,
                                    ))
                                    .ok();
                            }

                            let mut responses = OpenDeckConfigResponses::None;
                            ctx.shared.opendeck.lock(|opendeck| {
                                responses = opendeck.process_sysex(sysex_receive_buffer.as_ref());
                            });
                            if let Err(err) = ctx.local.sender.try_send(responses) {
                                match err {
                                    TrySendError::Full(_) => {
                                        error!("sysex out queue full");
                                    }
                                    TrySendError::NoReceiver(_) => {
                                        error!("sysex out queue has no receiver");
                                    }
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    error!("SysEx buffer overflow");
                    break;
                }
            }
        }
    }

    #[task(shared = [opendeck])]
    async fn sysex_processor(
        mut ctx: sysex_processor::Context,
        mut receiver: Receiver<'static, OpenDeckConfigResponses, SYSEX_CAPACITY>,
        mut sender: Sender<'static, UsbMidiEventPacket, USB_OUT_CAPACITY>,
    ) {
        #[cfg(feature = "opendeck")]
        {
            while let Ok(responses) = &mut receiver.recv().await {
                let output_buffer = &mut [0u8; 78];
                let mut more = true;
                let mut len;
                while more {
                    len = 0;
                    ctx.shared.opendeck.lock(|opendeck| {
                        match responses.next(output_buffer, &mut opendeck.config) {
                            Ok(Some(response)) => {
                                len = response.data().len();
                            }
                            Ok(None) => {
                                more = false;
                            }
                            Err(_) => {
                                more = false;
                                error!("SysEx buffer overflow");
                            }
                        }
                    });
                    if len == 0 {
                        continue;
                    }
                    debug!("SysEx OUT message: {:?}", output_buffer[..len]);
                    for chunk in output_buffer[..len].chunks(3) {
                        let packet =
                            UsbMidiEventPacket::try_from_payload_bytes(CableNumber::Cable0, chunk);
                        match packet {
                            Ok(packet) => {
                                sender.send(packet).await.unwrap();
                            }
                            Err(_) => {
                                error!("USB MIDI packet error");
                            }
                        }
                    }
                }
            }
        }
        #[cfg(not(feature = "opendeck"))]
        {
            let _ = (&mut ctx, &mut sender);
            loop {
                let _ = receiver.recv().await;
            }
        }
    }

    #[task(local = [eeprom_i2c], shared = [opendeck, pe_config, global_config, active_preset, state_store, presets_skipped])]
    async fn persist(
        mut ctx: persist::Context,
        mut receiver: Receiver<'static, pedalboard_midi::persist::PersistCommand, PERSIST_CAPACITY>,
        mut status_sender: Sender<'static, SystemStatus, SYSTEM_STATUS_CAPACITY>,
    ) {
        let eeprom = ctx.local.eeprom_i2c;
        info!("config persistence: loading from flash");
        if let Some(mut store) = pedalboard_midi::storage::ConfigStore::try_new() {
            // Load persisted config and replay as SET commands
            #[cfg(feature = "opendeck")]
            {
                let entries = store.load_all().await;
                if !entries.is_empty() {
                    info!("restoring {} config entries from flash", entries.len());
                    ctx.shared.opendeck.lock(|opendeck| {
                        let mut buf = [0u8; 78];
                        for &(block, section, index, value) in &entries {
                            if block == 7 {
                                continue; // legacy label entries (removed), skip
                            }
                            let raw = [
                                0xF0,
                                0x00,
                                0x53,
                                0x43,
                                0x00,
                                0x00,
                                0x01,
                                0x00,
                                block,
                                section,
                                (index >> 7) & 0x7F,
                                index & 0x7F,
                                ((value >> 8) as u8) & 0x7F,
                                (value as u8) & 0x7F,
                                0xF7,
                            ];
                            let mut responses = opendeck.config.process_sysex(&raw);
                            while let Ok(Some(_)) = responses.next(&mut buf, &mut opendeck.config) {
                            }
                        }
                    });
                }
            }
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
                        postcard::from_bytes::<pedalboard_protocol::config::Preset>(payload)
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
                .load_preset(
                    pedalboard_protocol::config::GLOBAL_CONFIG_RESOURCE,
                    &mut gc_buf,
                )
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
                    postcard::from_bytes::<pedalboard_protocol::config::GlobalConfig>(&data[1..])
                {
                    info!("global config loaded from flash");
                    ctx.shared.global_config.lock(|g| *g = gc);
                }
            }

            // Enter persist loop
            while let Ok(cmd) = receiver.recv().await {
                use pedalboard_midi::persist::PersistCommand;
                match cmd {
                    #[cfg(feature = "opendeck")]
                    PersistCommand::Save(block, section, index, value) => {
                        store.save(block, section, index, value).await;
                    }
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
                            if preset_index == pedalboard_protocol::config::GLOBAL_CONFIG_RESOURCE {
                                info!("global config cleared");
                                ctx.shared.global_config.lock(|g| *g = Default::default());
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
                                            pedalboard_protocol::config::Preset::default();
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
                        } else if preset_index
                            == pedalboard_protocol::config::GLOBAL_CONFIG_RESOURCE
                        {
                            // Global config — apply and save to flash
                            if let Ok(gc) = postcard::from_bytes::<
                                pedalboard_protocol::config::GlobalConfig,
                            >(&data)
                            {
                                info!("global config applied and saved");
                                ctx.shared.global_config.lock(|g| *g = gc);
                            }
                            store.save_preset(preset_index, &versioned).await;
                        } else if let Ok(preset) =
                            postcard::from_bytes::<pedalboard_protocol::config::Preset>(&data)
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
                                        .push(pedalboard_protocol::config::Preset::default())
                                        .ok();
                                }
                                cfg.presets[idx] = preset;
                            });
                            store.save_preset(preset_index, &versioned).await;
                            // Write initial state from preset defaults to EEPROM
                            let buf = ctx.shared.pe_config.lock(|cfg| {
                                let mut state_store =
                                    pedalboard_protocol::state::PresetStateStore::new();
                                for (i, p) in cfg.presets.iter().enumerate() {
                                    if i >= pedalboard_protocol::state::EEPROM_MAX_PRESETS {
                                        break;
                                    }
                                    if !p.defaults.button_active.is_empty()
                                        || !p.defaults.encoder_values.is_empty()
                                    {
                                        state_store.set_state(
                                            i,
                                            pedalboard_protocol::state::PresetState::from_defaults(
                                                p,
                                            ),
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
                        let buf = pedalboard_protocol::state::PresetStateStore::cleared_eeprom();
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

    #[task(local = [led_spi, leds: pedalboard_midi::leds::Leds = pedalboard_midi::leds::Leds::new()])]
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

    #[task(local = [displays], shared = [active_preset, opendeck, pe_config])]
    async fn display_out(
        mut ctx: display_out::Context,
        mut receiver: Receiver<'static, [u8; 3], DISPLAY_LOG_CAPACITY>,
        mut event_receiver: Receiver<'static, pedalboard_midi::pe_handler::DisplayEvent, 4>,
        mut system_status_receiver: Receiver<'static, SystemStatus, SYSTEM_STATUS_CAPACITY>,
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

            // Check if SysEx session switched mode
            #[cfg(feature = "opendeck")]
            let sysex_active = ctx.shared.opendeck.lock(|od| od.config.sysex_enabled());
            #[cfg(not(feature = "opendeck"))]
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
            if config_changed && !debug_mode {
                let idx = (current_preset as usize) % presets.len();
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
                                SystemAction::TapTempo => "Tap",
                                SystemAction::SetBpm(_) => "BPM",
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
                                        pedalboard_protocol::config::EncoderAction::Cc { cc, .. } => Some(*cc as u8),
                                        pedalboard_protocol::config::EncoderAction::CcRelative { cc, .. } => Some(*cc),
                                        _ => None,
                                    };
                                    if enc_cc == Some(raw[1]) {
                                        let lbl = if enc.label.is_empty() {
                                            String::try_from("Vol").unwrap_or_default()
                                        } else {
                                            enc.label.clone()
                                        };
                                        displays.draw_overlay(DisplayLocation::L, lbl.as_str(), raw[2]);
                                        overlay_ticks = OVERLAY_DURATION;
                                        show_overlay = true;
                                        return;
                                    }
                                }
                                // Check encoder 1
                                if let Some(enc) = preset.and_then(|p| p.encoders.get(1)) {
                                    let enc_cc = match &enc.action {
                                        pedalboard_protocol::config::EncoderAction::Cc { cc, .. } => Some(*cc as u8),
                                        pedalboard_protocol::config::EncoderAction::CcRelative { cc, .. } => Some(*cc),
                                        _ => None,
                                    };
                                    if enc_cc == Some(raw[1]) {
                                        let lbl = if enc.label.is_empty() {
                                            String::try_from("Gain").unwrap_or_default()
                                        } else {
                                            enc.label.clone()
                                        };
                                        displays.draw_overlay(DisplayLocation::R, lbl.as_str(), raw[2]);
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
                                        displays.draw_overlay(DisplayLocation::L, lbl.as_str(), raw[2]);
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
                                        displays.draw_overlay(DisplayLocation::R, lbl.as_str(), raw[2]);
                                        overlay_ticks = OVERLAY_DURATION;
                                        show_overlay = true;
                                        return;
                                    }
                                }
                                // Fallback: hardcoded CC#0/1/2/3 for OpenDeck default mode
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

    #[task(shared = [global_config])]
    async fn midi_clock(
        mut ctx: midi_clock::Context,
        mut sender: Sender<'static, UsbMidiEventPacket, USB_OUT_CAPACITY>,
        mut din_sender: Sender<'static, [u8; 3], DIN_THRU_CAPACITY>,
        mut led_sender: Sender<'static, LedEvent, LED_CAPACITY>,
    ) {
        loop {
            let interval_us = ctx.shared.global_config.lock(|gc| {
                if gc.midi_clock {
                    Some(gc.tick_interval_us())
                } else {
                    None
                }
            });
            match interval_us {
                Some(us) => {
                    // Send MIDI Clock (0xF8)
                    let raw: [u8; 3] = [0xF8, 0x00, 0x00];
                    din_sender.try_send(raw).ok();
                    if let Ok(packet) =
                        UsbMidiEventPacket::try_from_payload_bytes(CableNumber::Cable0, &[0xF8])
                    {
                        sender.try_send(packet).ok();
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
        cfg: &pedalboard_protocol::config::Config,
    ) {
        for (i, meta) in presets.iter_mut().enumerate() {
            let (name, labels) =
                pedalboard_midi::views::performance::preset_meta_from_config(cfg, i);
            meta.name = name;
            meta.button_labels = labels;
        }
    }
}

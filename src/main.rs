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
    use pedalboard_midi::leds::LedData;
    use pedalboard_midi::opendeck_handler::{OpenDeck, OpenDeckConfigResponses};
    use crate::Mono;
    use core::mem::MaybeUninit;
    use defmt::{debug, error, info, warn};
    use embedded_hal::digital::OutputPin;
    use embedded_hal_bus::i2c::AtomicDevice;
    use embedded_hal_bus::util::AtomicCell;
    use rtic_sync::channel::{Receiver, Sender, TrySendError};
    use rtic_sync::make_channel;

    use heapless::Vec;
    use midi2::Data;
    use midi_convert::midi_types::MidiMessage;
    use midi_convert::{parse::MidiTryParseSlice, render_slice::MidiRenderSlice};
    use rp2040_hal::{
        adc::{Adc, AdcPin},
        clocks::init_clocks_and_plls,
        fugit::{HertzU32, RateExtU32},
        gpio::{
            bank0::{
                Gpio0, Gpio1, Gpio10, Gpio11, Gpio16, Gpio17, Gpio18, Gpio19,
                Gpio2, Gpio20, Gpio21, Gpio24, Gpio25, Gpio3, Gpio4, Gpio5, Gpio6, Gpio7,
            },
            FunctionI2C, FunctionSio, FunctionUart, Pin, Pins, PullDown, PullUp,
            SioInput, SioOutput,
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
        led_sender_midi: Sender<'static, LedData, LED_CAPACITY>,
        led_sender_usb: Sender<'static, LedData, LED_CAPACITY>,
        mon_sender_midi: Sender<'static, (), 1>,
        mon_sender_usb: Sender<'static, (), 1>,
        usb_sender_din_thru: Sender<'static, UsbMidiEventPacket, USB_OUT_CAPACITY>,
        usb_sender_usb_thru: Sender<'static, UsbMidiEventPacket, USB_OUT_CAPACITY>,
        din_thru_receiver: Receiver<'static, [u8; 3], DIN_THRU_CAPACITY>,
        din_thru_sender: Sender<'static, [u8; 3], DIN_THRU_CAPACITY>,
        persist_sender: Sender<'static, pedalboard_midi::opendeck_handler::PersistCommand, PERSIST_CAPACITY>,
    }
    const USB_OUT_CAPACITY: usize = 32;
    const SYSEX_CAPACITY: usize = 1;
    const DISPLAY_LOG_CAPACITY: usize = 8;
    const LED_CAPACITY: usize = 1;
    const DIN_THRU_CAPACITY: usize = 8;
    const PERSIST_CAPACITY: usize = pedalboard_midi::opendeck_handler::PERSIST_CAPACITY;

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

        let exp_a_pin = AdcPin::new(pins.gpio27.into_floating_input()).unwrap();
        let exp_b_pin = AdcPin::new(pins.gpio28.into_floating_input()).unwrap();
        let exp = ExpressionPedals::new_direct(adc, exp_a_pin, exp_b_pin);
        let inputs = Inputs::new(vol, gain, buttons, exp);

        // Configure PIO for Ws2812 LEDs
        let led_pin: Pin<Gpio11, rp2040_hal::gpio::FunctionPio0, PullDown> =
            pins.gpio11.into_function();
        let (mut pio0, sm0, _, _, _) = ctx.device.PIO0.split(&mut resets);
        let led_spi = Ws2812Direct::new(
            led_pin,
            &mut pio0,
            sm0,
            clocks.peripheral_clock.freq(),
        );

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

        // Read AT24CS01 128-bit unique serial number if EEPROM is populated (v4.0+ boards)
        // Security register device address: 0x58, serial at word address 0x80
        let mut serial_number = [0u8; 16];
        let eeprom_serial_ok = {
            use embedded_hal::i2c::I2c;
            i2c.write_read(0x58u8, &[0x80u8], &mut serial_number).is_ok()
        };

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

        let (led_sender, led_receiver) = make_channel!(LedData, LED_CAPACITY);
        let (mon_sender, mon_receiver) = make_channel!((), 1);
        let (din_thru_sender, din_thru_receiver) = make_channel!([u8; 3], DIN_THRU_CAPACITY);
        let (persist_sender, persist_receiver) = make_channel!(pedalboard_midi::opendeck_handler::PersistCommand, PERSIST_CAPACITY);

        blink::spawn().unwrap();
        led_writer::spawn(led_receiver).unwrap();
        mon_off::spawn(mon_receiver, led_sender.clone()).unwrap();
        poll_input::spawn(usb_sender.clone(), display_sender, led_sender.clone()).unwrap();
        display_out::spawn(display_receiver).unwrap();
        persist::spawn(persist_receiver).unwrap();

        let mut opendeck = OpenDeck::new(
            opendeck::config::FirmwareVersion {
                major: 1,
                minor: 0,
                revision: 0,
            },
            0x123456,
            persist_sender.clone(),
        );
        if eeprom_serial_ok {
            opendeck.config.set_serial_number(&serial_number);
        }

        info!("pedalboard-midi initialized");
        (
            Shared {
                usb_midi,
                usb_dev,
                opendeck,
                active_preset: 0,
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
                mon_sender_midi: mon_sender.clone(),
                mon_sender_usb: mon_sender,
                usb_sender_din_thru: usb_sender.clone(),
                usb_sender_usb_thru: usb_sender.clone(),
                din_thru_receiver,
                din_thru_sender,
                persist_sender,
            },
        )
    }

    #[task(binds = UART0_IRQ, local = [uart_midi_in, led_sender_midi, mon_sender_midi, usb_sender_din_thru], shared = [opendeck])]
    fn midi_in(mut ctx: midi_in::Context) {
        use midi2::prelude::*;

        match ctx.local.uart_midi_in.read() {
            Ok(m) => {
                let (led_data, din_to_usb) = ctx.shared.opendeck.lock(|opendeck| {
                    let mut buf = [0x00u8; 3];
                    m.render_slice(&mut buf);
                    let thru = opendeck.din_to_usb_thru();
                    let ld = if let Ok(m) = BytesMessage::try_from(&buf[..]) {
                        Some(opendeck.handle_midi_input(&m))
                    } else {
                        None
                    };
                    (ld, if thru { Some(buf) } else { None })
                });
                if let Some(data) = led_data {
                    ctx.local.led_sender_midi.try_send(data).ok();
                    ctx.local.mon_sender_midi.try_send(()).ok();
                }
                if let Some(raw) = din_to_usb {
                    let packet = UsbMidiEventPacket::try_from_payload_bytes(
                        CableNumber::Cable0,
                        &raw,
                    );
                    if let Ok(packet) = packet {
                        ctx.local.usb_sender_din_thru.try_send(packet).ok();
                    }
                }
            }
            Err(nb::Error::WouldBlock) => {}
            Err(_) => error!("failed to receive midi message"),
        }
    }

    #[task(local = [inputs, uart_midi_out, din_thru_receiver], shared = [opendeck, active_preset])]
    async fn poll_input(
        mut ctx: poll_input::Context,
        mut sender: Sender<'static, UsbMidiEventPacket, USB_OUT_CAPACITY>,
        mut display_sender: Sender<'static, [u8; 3], DISPLAY_LOG_CAPACITY>,
        mut led_sender: Sender<'static, LedData, LED_CAPACITY>,
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
        ctx.shared.opendeck.lock(|opendeck| {
            use opendeck::{Amount, Block, OpenDeckRequest, Wish};
            use opendeck::encoder::EncoderSection;
            for i in 0..2u16 {
                opendeck.config.process_req(OpenDeckRequest::Configuration(
                    Wish::Set, Amount::Single,
                    Block::Encoder(i, EncoderSection::RepeatedValue(0)),
                ));
            }
            opendeck.reset_encoder_rings();
        });

        let mut lp_d = pedalboard_midi::long_press::LongPressDetector::new();
        let mut lp_f = pedalboard_midi::long_press::LongPressDetector::new();

        loop {
            // Drain USB→DIN thru messages
            while let Ok(raw) = din_thru_receiver.try_recv() {
                if let Ok(mm) = MidiMessage::try_parse_slice(&raw) {
                    uart_midi_out.write(&mm).ok();
                }
            }

            let mut events = heapless::Vec::<_, 14>::new();
            inputs.poll_encoders(&mut events);
            let slow_events = inputs.update();
            for e in slow_events.iter() {
                events.push(*e).ok();
            }

            // Long-press detection for D (prev preset) and F (next preset)
            use pedalboard_midi::events::InputEvent;
            use pedalboard_midi::long_press::Gesture;
            let edge_d = events.iter().find_map(|e| match e {
                InputEvent::ButtonD(edge) => Some(*edge),
                _ => None,
            });
            let edge_f = events.iter().find_map(|e| match e {
                InputEvent::ButtonF(edge) => Some(*edge),
                _ => None,
            });
            let gesture_d = lp_d.update(edge_d);
            let gesture_f = lp_f.update(edge_f);

            // Handle preset switching on long press
            let mut preset_changed = false;
            if gesture_d == Some(Gesture::LongPress) || gesture_f == Some(Gesture::LongPress) {
                preset_changed = true;
            }

            // Filter out D/F events if long-press is active (suppress normal MIDI)
            let suppress_d = lp_d.is_active();
            let suppress_f = lp_f.is_active();
            events.retain(|e| match e {
                InputEvent::ButtonD(_) if suppress_d => false,
                InputEvent::ButtonF(_) if suppress_f => false,
                _ => true,
            });

            let mut all_sent: heapless::Vec<[u8; 3], 8> = heapless::Vec::new();
            let mut led_data: Option<LedData> = None;
            let mut din_enabled = true;
            let mut component_info_buf: Option<([u8; 16], usize)> = None;
            if !events.is_empty() || preset_changed {
                if preset_changed {
                    ctx.shared.active_preset.lock(|active_preset| {
                        let current = *active_preset;
                        let next = if gesture_f == Some(Gesture::LongPress) {
                            (current + 1) % 32
                        } else {
                            if current == 0 { 31 } else { current - 1 }
                        };
                        *active_preset = next;
                    });
                }
                ctx.shared.opendeck.lock(|opendeck| {
                    din_enabled = opendeck.din_midi_enabled();
                    for event in events {
                        // Capture component info for the last event (avoids flooding)
                        let mut ci_buf = [0u8; 16];
                        if let Some(len) = opendeck.component_info(&event, &mut ci_buf) {
                            component_info_buf = Some((ci_buf, len));
                        }
                        let mut messages = opendeck.handle_human_input(event);
                        let mut buf = [0x00u8; 6];
                        while let Ok(Some(m)) = messages.next(&mut buf) {
                            let data = m.data();
                            if data.len() >= 3 {
                                let mut raw = [0u8; 3];
                                raw.copy_from_slice(&data[..3]);
                                all_sent.push(raw).ok();
                            }
                        }
                    }
                    for raw in &all_sent {
                        led_data = Some(opendeck.notify_local_midi(raw));
                    }
                });
            }
            // Send LED update outside the lock
            if let Some(data) = led_data {
                led_sender.try_send(data).ok();
            }
            // Send MIDI outside the lock
            for raw in &all_sent {
                // DIN MIDI out (only if enabled)
                if din_enabled {
                    if let Ok(mm) = MidiMessage::try_parse_slice(raw) {
                        uart_midi_out.write(&mm).ok();
                    }
                }
                // Display log (always for locally-generated messages)
                display_sender.try_send(*raw).ok();
                // USB MIDI out (always for locally-generated messages)
                let packet = UsbMidiEventPacket::try_from_payload_bytes(
                    CableNumber::Cable0,
                    raw,
                );
                if let Ok(packet) = packet {
                    sender.try_send(packet).ok();
                }
            }
            // Send component info SysEx (chunked into 3-byte USB MIDI packets)
            if let Some((ci_buf, ci_len)) = component_info_buf {
                for chunk in ci_buf[..ci_len].chunks(3) {
                    if let Ok(packet) = UsbMidiEventPacket::try_from_payload_bytes(
                        CableNumber::Cable0,
                        chunk,
                    ) {
                        sender.try_send(packet).ok();
                    }
                }
            }
            Mono::delay(1.millis()).await;
        }
    }

    #[task(binds = USBCTRL_IRQ, priority = 3,
        local = [ buf: Vec::<u8, 64>=Vec::new(), sender, led_sender_usb, mon_sender_usb, usb_sender_usb_thru, din_thru_sender, persist_sender],
        shared =[usb_midi,usb_dev,opendeck]
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
                if let Ok(m) = midi2::BytesMessage::try_from(packet.payload_bytes()) {
                    let (led_data, usb_to_din, usb_to_usb) = ctx.shared.opendeck.lock(|opendeck| {
                        let to_din = opendeck.usb_to_din_thru();
                        let to_usb = opendeck.usb_to_usb_thru();
                        let ld = opendeck.handle_midi_input(&m);
                        (ld, to_din, to_usb)
                    });
                    ctx.local.led_sender_usb.try_send(led_data).ok();
                    ctx.local.mon_sender_usb.try_send(()).ok();
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

                        // Detect SET SINGLE commands and persist them
                        // Format: F0 00 53 43 00 PP 01 00 BLOCK SECTION IDX_H IDX_L VAL_H VAL_L F7
                        if sysex_receive_buffer.len() >= 15
                            && sysex_receive_buffer[6] == 0x01  // WISH = SET
                            && sysex_receive_buffer[7] == 0x00  // AMOUNT = SINGLE
                        {
                            let block = sysex_receive_buffer[8];
                            let section = sysex_receive_buffer[9];
                            let index = sysex_receive_buffer[11]; // LSB
                            // Store raw two-byte value as-is (high << 8 | low)
                            let value = ((sysex_receive_buffer[12] as u16) << 8)
                                | sysex_receive_buffer[13] as u16;
                            ctx.local.persist_sender.try_send(pedalboard_midi::opendeck_handler::PersistCommand::Save(block, section, index, value)).ok();
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

    #[task(shared = [opendeck])]
    async fn persist(
        mut ctx: persist::Context,
        mut receiver: Receiver<'static, pedalboard_midi::opendeck_handler::PersistCommand, PERSIST_CAPACITY>,
    ) {
        info!("config persistence: loading from flash");
        if let Some(mut store) = pedalboard_midi::storage::ConfigStore::try_new() {
            // Load persisted config and replay as SET commands
            let entries = store.load_all().await;
            if !entries.is_empty() {
                info!("restoring {} config entries from flash", entries.len());
                ctx.shared.opendeck.lock(|opendeck| {
                    use opendeck::{Amount, OpenDeckRequest, Wish};
                    let mut buf = [0u8; 78];
                    for &(block, section, index, value) in &entries {
                        let raw = [
                            0xF0, 0x00, 0x53, 0x43, 0x00, 0x00, 0x01, 0x00,
                            block, section,
                            (index >> 7) & 0x7F, index & 0x7F,
                            ((value >> 8) as u8) & 0x7F, (value as u8) & 0x7F,
                            0xF7,
                        ];
                        let mut responses = opendeck.config.process_sysex(&raw);
                        // Must consume iterator to trigger process_req
                        while let Ok(Some(_)) = responses.next(&mut buf, &mut opendeck.config) {}
                    }
                });
            }
            info!("config persistence ready");
            // Enter persist loop
            while let Ok(cmd) = receiver.recv().await {
                use pedalboard_midi::opendeck_handler::PersistCommand;
                match cmd {
                    PersistCommand::Save(block, section, index, value) => {
                        store.save(block, section, index, value).await;
                    }
                    PersistCommand::EraseAll => {
                        store.erase_all().await;
                        info!("factory reset: storage erased, rebooting");
                        cortex_m::peripheral::SCB::sys_reset();
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

    #[task(local = [led_spi])]
    async fn led_writer(
        ctx: led_writer::Context,
        mut receiver: Receiver<'static, LedData, LED_CAPACITY>,
    ) {
        while let Ok(data) = receiver.recv().await {
            ctx.local
                .led_spi
                .write(brightness(data.iter().cloned(), 8))
                .unwrap();
        }
    }

    #[task(shared = [opendeck])]
    async fn mon_off(
        mut ctx: mon_off::Context,
        mut receiver: Receiver<'static, (), 1>,
        mut led_sender: Sender<'static, LedData, LED_CAPACITY>,
    ) {
        while let Ok(()) = receiver.recv().await {
            Mono::delay(100.millis()).await;
            // Drain any extra signals that arrived during the delay
            while receiver.try_recv().is_ok() {}
            let data = ctx.shared.opendeck.lock(|opendeck| opendeck.clear_mon());
            led_sender.try_send(data).ok();
        }
    }

    #[task(local = [displays], shared = [active_preset])]
    async fn display_out(
        mut ctx: display_out::Context,
        mut receiver: Receiver<'static, [u8; 3], DISPLAY_LOG_CAPACITY>,
    ) {
        use crate::hmi::display::DisplayLocation;
        use heapless::String;
        use pedalboard_midi::views::performance::PresetMeta;

        let displays = ctx.local.displays;
        displays.splash_screen();
        Mono::delay(2000.millis()).await;

        // Placeholder preset metadata — will come from flash config later
        let mut presets: [PresetMeta; 3] = core::array::from_fn(|_| PresetMeta::default());
        presets[0].name = String::try_from("Preset 1").unwrap_or_default();
        presets[0].button_labels[0] = String::try_from("Drive").unwrap_or_default();
        presets[0].button_labels[1] = String::try_from("Delay").unwrap_or_default();
        presets[0].button_labels[2] = String::try_from("Reverb").unwrap_or_default();
        presets[0].button_labels[3] = String::try_from("Looper").unwrap_or_default();
        presets[0].button_labels[4] = String::try_from("Tap").unwrap_or_default();
        presets[0].button_labels[5] = String::try_from("Bank+").unwrap_or_default();

        presets[1].name = String::try_from("Preset 2").unwrap_or_default();
        presets[1].button_labels[0] = String::try_from("Fuzz").unwrap_or_default();
        presets[1].button_labels[1] = String::try_from("Chorus").unwrap_or_default();
        presets[1].button_labels[2] = String::try_from("Trem").unwrap_or_default();
        presets[1].button_labels[3] = String::try_from("Phaser").unwrap_or_default();
        presets[1].button_labels[4] = String::try_from("Wah").unwrap_or_default();
        presets[1].button_labels[5] = String::try_from("Bank-").unwrap_or_default();

        presets[2].name = String::try_from("Preset 3").unwrap_or_default();
        presets[2].button_labels[0] = String::try_from("Clean").unwrap_or_default();
        presets[2].button_labels[1] = String::try_from("Boost").unwrap_or_default();
        presets[2].button_labels[2] = String::try_from("Hall").unwrap_or_default();
        presets[2].button_labels[3] = String::try_from("Loop").unwrap_or_default();
        presets[2].button_labels[4] = String::try_from("Tune").unwrap_or_default();
        presets[2].button_labels[5] = String::try_from("Bank-").unwrap_or_default();

        let mut current_preset: u8 = 0;
        displays.draw_performance(&presets[0]);

        // Overlay timeout: counts down each loop iteration (200ms each)
        let mut overlay_ticks: u8 = 0;
        const OVERLAY_DURATION: u8 = 5; // ~1s
        const PRESET_OVERLAY_DURATION: u8 = 10; // ~2s

        loop {
            let mut show_overlay = false;

            // Check for preset change
            let new_preset = ctx.shared.active_preset.lock(|p| *p);
            if new_preset != current_preset {
                current_preset = new_preset;
                let idx = (current_preset as usize) % presets.len();
                displays.draw_preset_overlay(current_preset + 1, presets[idx].name.as_str());
                overlay_ticks = PRESET_OVERLAY_DURATION;
                show_overlay = true;
            }

            while let Ok(raw) = receiver.try_recv() {
                let status = raw[0] & 0xF0;
                let ch = (raw[0] & 0x0F) + 1;
                match status {
                    0x90 => {},
                    0x80 => {},
                    0xB0 => {
                        // Encoder overlay: CC#0 = Vol (left), CC#1 = Gain (right)
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
                            _ => {}
                        }
                    }
                    _ => {},
                }
            }

            if !show_overlay && overlay_ticks > 0 {
                overlay_ticks -= 1;
                if overlay_ticks == 0 {
                    let idx = (current_preset as usize) % presets.len();
                    displays.draw_performance(&presets[idx]);
                }
            }

            Mono::delay(200.millis()).await;
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

}

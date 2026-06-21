#![no_std]
#![no_main]

mod handler;
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

    use crate::handler::Handler;
    use pedalboard_midi::opendeck_handler::OpenDeckConfigResponses;
    use crate::hmi::inputs::{Buttons, ExpressionPedals, Inputs, Rotary};
    use crate::Mono;
    use core::mem::MaybeUninit;
    use defmt::{debug, error, info};
    use embedded_hal::{digital::OutputPin, spi::MODE_0};
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
                Gpio0, Gpio1, Gpio10, Gpio11, Gpio12, Gpio14, Gpio16, Gpio17, Gpio18, Gpio19,
                Gpio2, Gpio20, Gpio21, Gpio24, Gpio25, Gpio3, Gpio4, Gpio5, Gpio6, Gpio7,
            },
            FunctionI2C, FunctionSio, FunctionSpi, FunctionUart, Pin, Pins, PullDown, PullUp,
            SioInput, SioOutput,
        },
        i2c::I2C,
        pac::{I2C0, UART0},
        spi::Spi,
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

    use ws2812_spi::Ws2812;

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

    type LedSpi = Spi<
        rp2040_hal::spi::Enabled,
        rp2040_hal::pac::SPI1,
        (
            Pin<Gpio11, FunctionSpi, PullDown>,
            Pin<Gpio12, FunctionSpi, PullDown>,
            Pin<Gpio14, FunctionSpi, PullDown>,
        ),
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
        handlers: crate::handler::Handlers,
    }

    #[local]
    struct Local {
        uart_midi_out: MidiOut,
        uart_midi_in: MidiIn,
        inputs: InputPins,
        led_spi: Ws2812<LedSpi>,
        displays: crate::hmi::display::Displays<
            AtomicDevice<'static, I2CBus>,
            AtomicDevice<'static, I2CBus>,
        >,
        debug_led: Pin<Gpio10, FunctionSio<SioOutput>, PullDown>,
        sender: Sender<'static, OpenDeckConfigResponses, SYSEX_CAPACITY>,
    }
    const USB_OUT_CAPACITY: usize = 32;
    const SYSEX_CAPACITY: usize = 1;

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
        let exp_adc_fifo = adc
            .build_fifo()
            .clock_divider(0, 0)
            .round_robin((&exp_a_pin, &exp_b_pin))
            .start_paused();
        let exp = ExpressionPedals::new(exp_adc_fifo);
        let inputs = Inputs::new(vol, gain, buttons, exp);

        // Configure SPI for Ws2812 LEDs
        let spi_sclk = pins.gpio14.into_function::<FunctionSpi>();
        let spi_mosi = pins.gpio11.into_function::<FunctionSpi>();
        let spi_miso = pins.gpio12.into_function::<FunctionSpi>();
        let spi = Spi::<_, _, _, 8u8>::new(ctx.device.SPI1, (spi_mosi, spi_miso, spi_sclk)).init(
            &mut resets,
            &clocks.system_clock,
            3.MHz(),
            MODE_0,
        );
        let led_spi = Ws2812::new(spi);

        // Configure I²C for OLED display
        let sda_pin: Pin<_, FunctionI2C, PullUp> = pins.gpio24.reconfigure();
        let scl_pin: Pin<_, FunctionI2C, PullUp> = pins.gpio25.reconfigure();
        let i2c = I2C::i2c0(
            ctx.device.I2C0,
            sda_pin,
            scl_pin,
            400.kHz(),
            &mut resets,
            &clocks.system_clock,
        );
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

        blink::spawn().unwrap();
        led_animation::spawn().unwrap();
        poll_input::spawn(usb_sender.clone()).unwrap();
        display_out::spawn().unwrap();

        let handlers = crate::handler::Handlers::new();

        info!("pedalboard-midi initialized");
        (
            Shared {
                usb_midi,
                usb_dev,
                handlers,
            },
            Local {
                uart_midi_out,
                uart_midi_in,
                inputs,
                led_spi,
                displays,
                debug_led,
                sender: sysex_sender,
            },
        )
    }

    #[task(binds = UART0_IRQ, local = [uart_midi_in], shared = [handlers])]
    fn midi_in(mut ctx: midi_in::Context) {
        use midi2::prelude::*;

        match ctx.local.uart_midi_in.read() {
            Ok(m) => {
                ctx.shared.handlers.lock(|handlers| {
                    let mut buf = [0x00u8; 3];
                    m.render_slice(&mut buf);
                    if let Ok(m) = BytesMessage::try_from(&buf[..]) {
                        handlers.handle_midi_input(&m);
                    }
                });
            }
            Err(nb::Error::WouldBlock) => {}
            Err(_) => error!("failed to receive midi message"),
        }
    }

    #[task(local = [inputs, uart_midi_out], shared = [handlers])]
    async fn poll_input(
        mut ctx: poll_input::Context,
        mut sender: Sender<'static, UsbMidiEventPacket, USB_OUT_CAPACITY>,
    ) {
        let inputs = ctx.local.inputs;
        let uart_midi_out = ctx.local.uart_midi_out;
        loop {
            if let Some(event) = inputs.update() {
                ctx.shared.handlers.lock(|handlers| {
                    let mut messages = handlers.handle_human_input(event);
                    let mut more = true;
                    let mut buf = [0x00u8; 6];
                    while more {
                        match messages.next(&mut buf) {
                            Ok(Some(m)) => {
                                // always send to UART out
                                if let Ok(mm) = MidiMessage::try_parse_slice(m.data()) {
                                    debug!("sending midi message to MIDI-OUT {:?}", mm);
                                    uart_midi_out.write(&mm).ok();
                                }
                                let packet = UsbMidiEventPacket::try_from_payload_bytes(
                                    CableNumber::Cable0,
                                    m.data(),
                                );
                                match packet {
                                    Ok(packet) => {
                                        if let Err(err) = sender.try_send(packet) {
                                            match err {
                                                TrySendError::Full(_) => {
                                                    error!("USB MIDI out queue full");
                                                }
                                                TrySendError::NoReceiver(_) => {
                                                    error!("USB MIDI out queue has no receiver");
                                                }
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        error!("USB MIDI packet error");
                                    }
                                }
                            }
                            Ok(None) => {
                                more = false;
                            }
                            Err(_) => {
                                more = false;
                                error!("buffer overflow")
                            }
                        };
                    }
                });
            };
            // run this task once per millis
            Mono::delay(1.millis()).await;
        }
    }

    #[task(binds = USBCTRL_IRQ, priority = 3,
        local = [ buf: Vec::<u8, 64>=Vec::new(), sender],
        shared =[usb_midi,usb_dev,handlers]
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
                if let Ok(m) = midi2::BytesMessage::try_from(packet.as_raw_bytes()) {
                    ctx.shared.handlers.lock(|handlers| {
                        handlers.handle_midi_input(&m);
                    });
                }
                continue;
            }

            // packet containing a SysEx payload is detected, the data is saved
            // into a buffer and processed after the message is complete.
            if packet.is_sysex_start() {
                debug!("SysEx message start");
                sysex_receive_buffer.clear();
            }

            match sysex_receive_buffer.extend_from_slice(packet.payload_bytes()) {
                Ok(_) => {
                    if packet.is_sysex_end() {
                        debug!("SysEx IN  message: {:?}", sysex_receive_buffer);

                        // Process the SysEx message as request in a separate function
                        // and send an optional response back to the host.
                        let mut responses = OpenDeckConfigResponses::None;
                        ctx.shared.handlers.lock(|handlers| {
                            responses = handlers.process_sysex(sysex_receive_buffer.as_ref());
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

    #[task(shared = [handlers])]
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
                ctx.shared.handlers.lock(|handlers| {
                    match responses.next(output_buffer, handlers.config()) {
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

    #[task(local = [led_spi], shared =[handlers])]
    async fn led_animation(mut ctx: led_animation::Context) {
        loop {
            ctx.shared.handlers.lock(|handlers| {
                let data = handlers.leds().animate();
                ctx.local
                    .led_spi
                    .write(brightness(data.iter().cloned(), 8))
                    .unwrap();
            });
            // run this task with 20Hz
            Mono::delay(50.millis()).await;
        }
    }

    #[task(local = [displays])]
    async fn display_out(ctx: display_out::Context) {
        ctx.local
            .displays
            .show(crate::hmi::display::DisplayLocation::L);
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

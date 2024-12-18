#![no_std]
#![no_main]

mod devices;
mod handler;
mod hmi;
mod loudness;

use defmt_rtt as _;
use panic_probe as _;

use rtic::app;

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

    use crate::hmi::inputs::{ButtonPins, Inputs, RotaryPins};
    use defmt::*;
    use embedded_hal::{digital::v2::OutputPin, spi::MODE_0};

    use rp2040_hal::{
        adc::{Adc, AdcPin},
        clocks::init_clocks_and_plls,
        fugit::{HertzU32, RateExtU32, TimerDurationU64},
        gpio::{
            bank0::{Gpio0, Gpio1, Gpio10},
            FunctionI2C, FunctionSio, FunctionSpi, FunctionUart, Pin, Pins, PullDown, PullUp,
            SioOutput,
        },
        i2c::I2C,
        pac::UART0,
        rom_data::reset_to_usb_boot,
        spi::Spi,
        timer::{monotonic::Monotonic, Alarm0, Timer},
        uart::{DataBits, Reader, StopBits, UartConfig, UartPeripheral, Writer},
        usb::UsbBus,
        Clock, Sio, Watchdog,
    };
    use smart_leds::{brightness, SmartLedsWrite};
    use usb_device::{
        class_prelude::UsbBusAllocator,
        device::{UsbDeviceBuilder, UsbVidPid},
        prelude::UsbDeviceState,
    };
    use usbd_midi::{
        data::{
            usb::constants::{USB_AUDIO_CLASS, USB_MIDISTREAMING_SUBCLASS},
            usb_midi::{
                cable_number::CableNumber::Cable0, midi_packet_reader::MidiPacketBufferReader,
                usb_midi_event_packet::UsbMidiEventPacket,
            },
        },
        midi_device::MidiClass,
    };
    use ws2812_spi::Ws2812;

    type Duration = TimerDurationU64<1_000_000>;

    type MidiUartPins = (
        Pin<Gpio0, FunctionUart, PullDown>,
        Pin<Gpio1, FunctionUart, PullDown>,
    );
    type MidiOut = embedded_midi::MidiOut<Writer<UART0, MidiUartPins>>;
    type MidiIn = embedded_midi::MidiIn<Reader<UART0, MidiUartPins>>;

    #[monotonic(binds = TIMER_IRQ_0, default = true)]
    type MyMono = Monotonic<Alarm0>;

    #[shared]
    struct Shared {
        usb_midi: MidiClass<'static, UsbBus>,
        usb_dev: usb_device::device::UsbDevice<'static, UsbBus>,
        handlers: crate::handler::Handlers<crate::handler::dispatch::HandlerEnum>,
    }

    #[local]
    struct Local {
        uart_midi_out: MidiOut,
        uart_midi_in: MidiIn,
        inputs: Inputs,
        led_spi: crate::hmi::leds::LedDriver,
        display: crate::hmi::display::Display,
        debug_led: Pin<Gpio10, FunctionSio<SioOutput>, PullDown>,
    }

    #[init(local = [usb_bus: Option<usb_device::bus::UsbBusAllocator<UsbBus>> = None])]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        let mut resets = ctx.device.RESETS;
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

        let mut timer = Timer::new(ctx.device.TIMER, &mut resets, &clocks);
        let mono = Monotonic::new(timer, timer.alarm_0().unwrap());

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
        let usb_bus: &'static _ = ctx.local.usb_bus.insert(UsbBusAllocator::new(UsbBus::new(
            ctx.device.USBCTRL_REGS,
            ctx.device.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut resets,
        )));

        let usb_midi = MidiClass::new(usb_bus, 1, 1).unwrap();
        let usb_dev = UsbDeviceBuilder::new(usb_bus, UsbVidPid(0x2E8A, 0x0005))
            .manufacturer("github.com/pedalboard")
            .product("pedalboard-midi")
            .serial_number("0.0.1")
            .device_class(USB_AUDIO_CLASS)
            .device_sub_class(USB_MIDISTREAMING_SUBCLASS)
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
        let vol_pins = RotaryPins {
            clk: pins.gpio16.into_pull_up_input(),
            dt: pins.gpio17.into_pull_up_input(),
            sw: pins.gpio18.into_pull_up_input(),
        };

        let gain_pins = RotaryPins {
            clk: pins.gpio19.into_pull_up_input(),
            dt: pins.gpio20.into_pull_up_input(),
            sw: pins.gpio21.into_pull_up_input(),
        };

        let button_pins = ButtonPins(
            pins.gpio7.into_pull_up_input(),
            pins.gpio5.into_pull_up_input(),
            pins.gpio2.into_pull_up_input(),
            pins.gpio6.into_pull_up_input(),
            pins.gpio4.into_pull_up_input(),
            pins.gpio3.into_pull_up_input(),
        );

        // ADC for analog input
        let adc = Adc::new(ctx.device.ADC, &mut resets);
        let exp_a_pin = AdcPin::new(pins.gpio27.into_floating_input());
        let exp_b_pin = AdcPin::new(pins.gpio28.into_floating_input());

        let inputs = Inputs::new(vol_pins, gain_pins, button_pins, adc, exp_a_pin, exp_b_pin);

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
        let mut display = crate::hmi::display::Display::new(i2c);
        display.splash_screen();

        blink::spawn().unwrap();
        led_animation::spawn().unwrap();
        poll_input::spawn().unwrap();
        display_out::spawn_after(Duration::secs(1)).unwrap();

        let handlers = crate::handler::dispatch::create();

        info!("pedalboard-midi initialized");
        (
            Shared {
                usb_midi,
                usb_dev,
                handlers: crate::handler::Handlers::new(handlers),
            },
            Local {
                uart_midi_out,
                uart_midi_in,
                inputs,
                led_spi,
                display,
                debug_led,
            },
            init::Monotonics(mono),
        )
    }

    #[task(binds = UART0_IRQ, local = [uart_midi_in], shared = [handlers])]
    fn midi_in(mut ctx: midi_in::Context) {
        match ctx.local.uart_midi_in.read() {
            Ok(m) => {
                ctx.shared.handlers.lock(|handlers| {
                    handlers.process_midi_input(m);
                });
            }
            Err(nb::Error::WouldBlock) => {}
            Err(_) => error!("failed to receive midi message"),
        }
    }

    #[task(local = [uart_midi_out], shared = [usb_midi, usb_dev])]
    fn midi_out(mut ctx: midi_out::Context, messages: crate::handler::MidiMessages) {
        // always send to UART out
        let uart_midi_out = ctx.local.uart_midi_out;
        let msgs = messages.messages();
        for message in msgs.iter() {
            uart_midi_out.write(message).unwrap();
        }

        // optionally send to USB if a device is listening
        let configured = ctx
            .shared
            .usb_dev
            .lock(|usb_dev| usb_dev.state() == UsbDeviceState::Configured);
        if !configured {
            return;
        }
        for message in msgs.into_iter() {
            let p = UsbMidiEventPacket::from_midi(Cable0, message);
            ctx.shared.usb_midi.lock(|midi| match midi.send_message(p) {
                Ok(_) => debug!("message sent to usb"),
                Err(err) => error!("failed to send message: {}", err),
            });
        }
    }

    #[task(local = [inputs], shared = [handlers])]
    fn poll_input(mut ctx: poll_input::Context) {
        let inputs = ctx.local.inputs;

        if let Some(event) = inputs.update() {
            ctx.shared.handlers.lock(|handlers| {
                let actions = handlers.handle_human_input(event);
                if !actions.midi_messages.is_empty() {
                    midi_out::spawn(actions.midi_messages).unwrap();
                }
            });
        };
        // schedule to run this task once per millis
        poll_input::spawn_after(Duration::millis(1)).unwrap();
    }

    #[task(binds = USBCTRL_IRQ, priority = 3, local = [], shared =[usb_midi,usb_dev,handlers])]
    fn usb_rx(mut ctx: usb_rx::Context) {
        ctx.shared.usb_dev.lock(|usb_dev| {
            ctx.shared.usb_midi.lock(|usb_midi| {
                // Check for new data
                if !usb_dev.poll(&mut [usb_midi]) {
                    return;
                }

                let mut buffer = [0; 64];
                if let Ok(size) = usb_midi.read(&mut buffer) {
                    let buffer_reader = MidiPacketBufferReader::new(&buffer, size);
                    for packet in buffer_reader.flatten() {
                        match packet.message {
                            midi_types::MidiMessage::NoteOff(
                                midi_types::Channel::C16,
                                midi_types::Note::C1m,
                                ..,
                            ) => {
                                debug!("reset to usb boot");
                                reset_to_usb_boot(0, 0);
                            }
                            _ => {
                                ctx.shared.handlers.lock(|handlers| {
                                    handlers.process_midi_input(packet.message);
                                });
                            }
                        }
                    }
                }
            });
        });
    }

    #[task(local = [led_spi], shared =[handlers])]
    fn led_animation(mut ctx: led_animation::Context) {
        ctx.shared.handlers.lock(|handlers| {
            let data = handlers.leds().animate();
            ctx.local
                .led_spi
                .write(brightness(data.iter().cloned(), 8))
                .unwrap();
        });
        // schedule to run this task with 20Hz
        led_animation::spawn_after(Duration::millis(50)).unwrap();
    }

    #[task(local = [display])]
    fn display_out(ctx: display_out::Context) {
        ctx.local.display.show();
    }

    #[task(local = [debug_led, state: bool = false])]
    fn blink(ctx: blink::Context) {
        *ctx.local.state = !*ctx.local.state;
        if *ctx.local.state {
            ctx.local.debug_led.set_high().ok().unwrap();
        } else {
            ctx.local.debug_led.set_low().ok().unwrap();
        }
        blink::spawn_after(Duration::millis(500)).unwrap();
    }
}

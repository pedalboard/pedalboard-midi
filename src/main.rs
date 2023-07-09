#![no_std]
#![no_main]

mod devices;
mod hmi;
mod usb;

use defmt_rtt as _;
use panic_probe as _;
use rtic::app;

#[app(device = rp_pico::hal::pac, dispatchers = [SW0_IRQ])]
mod app {

    use crate::hmi::inputs::{ButtonPins, Inputs, RotaryPins};
    use defmt::*;
    use embedded_hal::digital::v2::OutputPin;
    use embedded_hal::spi::MODE_0;
    use fugit::HertzU32;
    use fugit::RateExtU32;

    use rp2040_monotonic::Rp2040Monotonic;
    use rp_pico::{
        hal::{
            adc::Adc,
            clocks::init_clocks_and_plls,
            gpio::{
                pin::bank0::{Gpio0, Gpio1, Gpio25},
                Function, FunctionSpi, FunctionUart, Pin, PushPullOutput, Uart,
            },
            rom_data::reset_to_usb_boot,
            spi::Spi,
            uart::{DataBits, Reader, StopBits, UartConfig, UartPeripheral, Writer},
            usb::UsbBus,
            Clock, Sio, Watchdog,
        },
        pac::SPI1,
        pac::UART0,
        Pins,
    };
    use smart_leds::{brightness, SmartLedsWrite};
    use usb_device::prelude::UsbDeviceState;
    use usb_device::{
        class_prelude::UsbBusAllocator,
        device::{UsbDeviceBuilder, UsbVidPid},
    };
    use usbd_midi::data::usb::constants::{USB_AUDIO_CLASS, USB_MIDISTREAMING_SUBCLASS};
    use usbd_midi::data::usb_midi::midi_packet_reader::MidiPacketBufferReader;
    use usbd_midi::midi_device::MidiClass;
    use ws2812_spi::Ws2812;

    type Duration = fugit::TimerDurationU64<1_000_000>;
    type MidiOut = embedded_midi::MidiOut<
        Writer<UART0, (Pin<Gpio0, Function<Uart>>, Pin<Gpio1, Function<Uart>>)>,
    >;
    type MidiIn = embedded_midi::MidiIn<
        Reader<UART0, (Pin<Gpio0, Function<Uart>>, Pin<Gpio1, Function<Uart>>)>,
    >;

    const SYS_HZ: u32 = 125_000_000_u32;

    #[monotonic(binds = TIMER_IRQ_0, default = true)]
    type Rp2040Mono = Rp2040Monotonic;

    #[shared]
    struct Shared {
        usb_midi: MidiClass<'static, UsbBus>,
        usb_dev: usb_device::device::UsbDevice<'static, UsbBus>,
        devices: crate::devices::Devices,
    }

    #[local]
    struct Local {
        led: Pin<Gpio25, PushPullOutput>,
        midi_out: MidiOut,
        midi_in: MidiIn,
        inputs: Inputs,
        ws: Ws2812<Spi<rp_pico::hal::spi::Enabled, SPI1, 8>>,
    }

    #[init(local = [usb_bus: Option<usb_device::bus::UsbBusAllocator<UsbBus>> = None])]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        let mut resets = cx.device.RESETS;
        let mut watchdog = Watchdog::new(cx.device.WATCHDOG);

        let clocks = init_clocks_and_plls(
            rp_pico::XOSC_CRYSTAL_FREQ,
            cx.device.XOSC,
            cx.device.CLOCKS,
            cx.device.PLL_SYS,
            cx.device.PLL_USB,
            &mut resets,
            &mut watchdog,
        )
        .ok()
        .unwrap();

        let timer = cx.device.TIMER;
        let mono = Rp2040Monotonic::new(timer);
        let sio = Sio::new(cx.device.SIO);
        let pins = Pins::new(
            cx.device.IO_BANK0,
            cx.device.PADS_BANK0,
            sio.gpio_bank0,
            &mut resets,
        );

        // Pins
        let led = pins.led.into_push_pull_output();

        // USB
        let usb_bus: &'static _ = cx.local.usb_bus.insert(UsbBusAllocator::new(UsbBus::new(
            cx.device.USBCTRL_REGS,
            cx.device.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut resets,
        )));

        let usb_midi = MidiClass::new(usb_bus, 1, 1).unwrap();
        let usb_dev = UsbDeviceBuilder::new(usb_bus, UsbVidPid(0x2E8A, 0x0005))
            .manufacturer("laenzlinger")
            .product("pedalboard-midi")
            .serial_number("0.0.1")
            .device_class(USB_AUDIO_CLASS)
            .device_sub_class(USB_MIDISTREAMING_SUBCLASS)
            .build();

        // UART Midi
        let uart_pins = (
            pins.gpio0.into_mode::<FunctionUart>(),
            pins.gpio1.into_mode::<FunctionUart>(),
        );
        let conf = UartConfig::new(
            HertzU32::from_raw(31250),
            DataBits::Eight,
            None,
            StopBits::One,
        );
        let uart = UartPeripheral::new(cx.device.UART0, uart_pins, &mut resets)
            .enable(conf, clocks.peripheral_clock.freq())
            .unwrap();
        let (mut rx, tx) = uart.split();
        rx.enable_rx_interrupt();
        let midi_out = MidiOut::new(tx);
        let midi_in = MidiIn::new(rx);

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
        let adc = Adc::new(cx.device.ADC, &mut resets);
        let exp_pin = pins.gpio28.into_floating_input();

        let inputs = Inputs::new(vol_pins, gain_pins, button_pins, adc, exp_pin);

        // These are implicitly used by the spi driver if they are in the correct mode
        let _spi_sclk = pins.gpio10.into_mode::<FunctionSpi>();
        let _spi_mosi = pins.gpio11.into_mode::<FunctionSpi>();
        let _spi_miso = pins.gpio12.into_mode::<FunctionSpi>();
        let spi = Spi::<_, _, 8>::new(cx.device.SPI1).init(
            &mut resets,
            SYS_HZ.Hz(),
            3_000_000u32.Hz(),
            &MODE_0,
        );

        let ws = Ws2812::new(spi);

        blink::spawn().unwrap();
        led_strip::spawn().unwrap();
        poll_input::spawn().unwrap();
        info!("pedalboard-midi initialized");
        (
            Shared {
                usb_midi,
                usb_dev,
                devices: crate::devices::Devices::default(),
            },
            Local {
                led,
                midi_out,
                midi_in,
                inputs,
                ws,
            },
            init::Monotonics(mono),
        )
    }

    #[task(local = [led, state: bool = false])]
    fn blink(ctx: blink::Context) {
        *ctx.local.state = !*ctx.local.state;
        if *ctx.local.state {
            ctx.local.led.set_high().ok().unwrap();
        } else {
            ctx.local.led.set_low().ok().unwrap();
        }
        blink::spawn_after(Duration::millis(500)).unwrap();
    }

    #[task(binds = UART0_IRQ, local = [midi_in], shared = [devices])]
    fn midi_in(mut ctx: midi_in::Context) {
        match ctx.local.midi_in.read() {
            Ok(m) => {
                ctx.shared.devices.lock(|devices| {
                    devices.process_midi_input(m);
                });
            }
            Err(nb::Error::WouldBlock) => {}
            Err(_) => error!("failed to receive midi message"),
        }
    }

    #[task(local = [midi_out], shared = [usb_midi, usb_dev])]
    fn midi_out(mut ctx: midi_out::Context, messages: crate::devices::MidiMessages) {
        let midi_out = ctx.local.midi_out;
        let msgs = messages.messages();
        for message in msgs.iter() {
            midi_out.write(message).unwrap();
        }

        let configured = ctx
            .shared
            .usb_dev
            .lock(|usb_dev| usb_dev.state() == UsbDeviceState::Configured);
        if !configured {
            return;
        }

        for message in msgs.into_iter() {
            if let Some(mm) = crate::usb::map_midi(message) {
                ctx.shared
                    .usb_midi
                    .lock(|midi| match midi.send_message(mm) {
                        Ok(_) => debug!("message sent to usb"),
                        Err(_) => error!("failed to send message"),
                    });
            }
        }
    }

    #[task(local = [inputs], shared = [devices])]
    fn poll_input(mut ctx: poll_input::Context) {
        let inputs = ctx.local.inputs;

        if let Some(event) = inputs.update() {
            ctx.shared.devices.lock(|devices| {
                let actions = devices.process_human_input(event);
                if !actions.midi_messages.is_empty() {
                    midi_out::spawn(actions.midi_messages).unwrap();
                }
            });
        };
        poll_input::spawn_after(Duration::millis(1)).unwrap();
    }

    #[task(binds = USBCTRL_IRQ, priority = 3, local = [], shared =[usb_midi,usb_dev,devices])]
    fn usb_rx(mut cx: usb_rx::Context) {
        cx.shared.usb_dev.lock(|usb_dev| {
            cx.shared.usb_midi.lock(|usb_midi| {
                // Check for new data
                if !usb_dev.poll(&mut [usb_midi]) {
                    return;
                }

                let mut buffer = [0; 64];
                if let Ok(size) = usb_midi.read(&mut buffer) {
                    let buffer_reader = MidiPacketBufferReader::new(&buffer, size);
                    for packet in buffer_reader.flatten() {
                        match packet.message {
                            usbd_midi::data::midi::message::Message::NoteOff(
                                usbd_midi::data::midi::channel::Channel::Channel16,
                                usbd_midi::data::midi::notes::Note::C1m,
                                ..,
                            ) => {
                                reset_to_usb_boot(0, 0);
                            }
                            usbd_midi::data::midi::message::Message::NoteOff(ch, note, vel) => {
                                cx.shared.devices.lock(|devices| {
                                    let vv: u8 = vel.into();
                                    devices.process_midi_input(midi_types::MidiMessage::NoteOff(
                                        midi_types::Channel::from(ch as u8),
                                        midi_types::Note::from(note as u8),
                                        midi_types::Value7::from(vv),
                                    ));
                                });
                            }
                            _ => {}
                        }
                    }
                }
            });
        });
    }

    #[task(local = [ws], shared =[devices])]
    fn led_strip(mut ctx: led_strip::Context) {
        ctx.shared.devices.lock(|devices| {
            let data = devices.leds().animate();
            ctx.local
                .ws
                .write(brightness(data.iter().cloned(), 32))
                .unwrap();
        });
        led_strip::spawn_after(Duration::millis(50)).unwrap();
    }
}

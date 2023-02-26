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
    use crate::hmi::leds::{Animation, Animations, Led, Leds};
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
            uart::{DataBits, StopBits, UartConfig, UartPeripheral, Writer},
            usb::UsbBus,
            Clock, Sio, Watchdog,
        },
        pac::SPI1,
        pac::UART0,
        Pins,
    };
    use smart_leds::colors::{GREEN, RED, WHITE};
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

    const SYS_HZ: u32 = 125_000_000_u32;

    #[monotonic(binds = TIMER_IRQ_0, default = true)]
    type Rp2040Mono = Rp2040Monotonic;

    #[shared]
    struct Shared {
        usb_midi: MidiClass<'static, UsbBus>,
        usb_dev: usb_device::device::UsbDevice<'static, UsbBus>,
    }

    #[local]
    struct Local {
        led: Pin<Gpio25, PushPullOutput>,
        midi_out: MidiOut,
        inputs: Inputs,
        devices: crate::devices::Devices,
        ws: Ws2812<Spi<rp_pico::hal::spi::Enabled, SPI1, 8>>,
        leds: Leds,
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
        let (_rx, tx) = uart.split();
        let midi_out = MidiOut::new(tx);

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
            pins.gpio2.into_pull_up_input(),
            pins.gpio3.into_pull_up_input(),
            pins.gpio4.into_pull_up_input(),
            pins.gpio5.into_pull_up_input(),
            pins.gpio6.into_pull_up_input(),
            pins.gpio7.into_pull_up_input(),
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

        let animations = Animations::with_only(Animation::On(Led::Mode, WHITE));
        led_strip::spawn(animations).unwrap();

        poll_input::spawn().unwrap();
        (
            Shared { usb_midi, usb_dev },
            Local {
                led,
                midi_out,
                inputs,
                devices: crate::devices::Devices::default(),
                ws,
                leds: crate::hmi::leds::Leds::default(),
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
        let color = if configured { GREEN } else { RED };
        let animations = Animations::with_only(Animation::Flash(Led::Mon, color));
        led_strip::spawn(animations).unwrap();

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

    #[task(local = [inputs, devices])]
    fn poll_input(ctx: poll_input::Context) {
        let inputs = ctx.local.inputs;
        let devices = ctx.local.devices;

        if let Some(event) = inputs.update() {
            let actions = devices.map(event);
            if !actions.midi_messages.is_empty() {
                midi_out::spawn(actions.midi_messages).unwrap();
            }
            if !actions.animations.is_empty() {
                led_strip::spawn(actions.animations).unwrap();
            }
        };
        poll_input::spawn_after(Duration::millis(1)).unwrap();
    }

    #[task(binds = USBCTRL_IRQ, priority = 3, local = [], shared =[usb_midi,usb_dev])]
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
                        #[allow(clippy::single_match)]
                        match packet.message {
                            usbd_midi::data::midi::message::Message::NoteOff(
                                usbd_midi::data::midi::channel::Channel::Channel16,
                                usbd_midi::data::midi::notes::Note::C1m,
                                ..,
                            ) => {
                                reset_to_usb_boot(0, 0);
                            }
                            _ => {}
                        }
                    }
                }
            });
        });
    }

    #[task(capacity = 5, local = [ws, leds])]
    fn led_strip(ctx: led_strip::Context, animations: Animations) {
        let leds = ctx.local.leds;
        for a in animations.animations() {
            let (data, next) = leds.animate(a);
            ctx.local
                .ws
                .write(brightness(data.iter().cloned(), 32))
                .unwrap();

            if next.is_some() {
                let next_animations = Animations::with_only(next.unwrap());
                led_strip::spawn_after(Duration::millis(50), next_animations).unwrap();
            }
        }
    }
}

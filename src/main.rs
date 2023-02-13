#![no_std]
#![no_main]

mod mmi;

use panic_halt as _;
use rtic::app;

#[app(device = rp_pico::hal::pac, dispatchers = [SW0_IRQ])]
mod app {

    use embedded_hal::digital::v2::OutputPin;
    use fugit::HertzU32;
    use rp2040_monotonic::Rp2040Monotonic;
    use rp_pico::{
        hal::{
            clocks::init_clocks_and_plls,
            gpio::{
                pin::bank0::{Gpio0, Gpio1, Gpio25},
                Function, FunctionUart, Pin, PushPullOutput, Uart,
            },
            rom_data::reset_to_usb_boot,
            uart::{DataBits, StopBits, UartConfig, UartPeripheral, Writer},
            usb::UsbBus,
            Clock, Sio, Watchdog,
        },
        pac::UART0,
        Pins,
    };
    use usb_device::{
        class_prelude::UsbBusAllocator,
        device::{UsbDeviceBuilder, UsbVidPid},
    };
    use usbd_serial::SerialPort;

    type Duration = fugit::TimerDurationU64<1_000_000>;
    type MidiOut = embedded_midi::MidiOut<
        Writer<UART0, (Pin<Gpio0, Function<Uart>>, Pin<Gpio1, Function<Uart>>)>,
    >;

    const XTAL_FREQ_HZ: u32 = 12_000_000;

    #[monotonic(binds = TIMER_IRQ_0, default = true)]
    type MyMono = Rp2040Monotonic;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        led: Pin<Gpio25, PushPullOutput>,
        usb_dev: usb_device::device::UsbDevice<'static, UsbBus>,
        serial: SerialPort<'static, UsbBus>,
        midi_out: MidiOut,
        inputs: crate::mmi::Inputs,
    }

    #[init(local = [usb_bus: Option<usb_device::bus::UsbBusAllocator<UsbBus>> = None])]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        let mut resets = cx.device.RESETS;
        let mut watchdog = Watchdog::new(cx.device.WATCHDOG);

        let clocks = init_clocks_and_plls(
            XTAL_FREQ_HZ,
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
        let serial = SerialPort::new(usb_bus);
        let usb_dev = UsbDeviceBuilder::new(usb_bus, UsbVidPid(0x2E8A, 0x0005))
            .manufacturer("laenzlinger")
            .product("pedalboard-midi")
            .serial_number("0.0.1")
            .device_class(2) // from: https://www.usb.org/defined-class-codes
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

        let pin = pins.gpio18.into_floating_input();

        let inputs = crate::mmi::Inputs::new(pin);

        blink::spawn().unwrap();
        (
            Shared {},
            Local {
                led,
                usb_dev,
                serial,
                midi_out,
                inputs,
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

    #[task(local = [midi_out])]
    fn midi_out(ctx: midi_out::Context) {
        let midi_out = ctx.local.midi_out;
        let message = embedded_midi::midi_types::MidiMessage::ControlChange(
            embedded_midi::midi_types::Channel::new(0),
            embedded_midi::midi_types::Control::new(0),
            embedded_midi::midi_types::Value7::new(0),
        );
        midi_out.write(&message).unwrap();
    }

    #[task(local = [inputs])]
    fn poll_input(ctx: poll_input::Context) {
        let inputs = ctx.local.inputs;

        match inputs.update() {
            Some(_event) => midi_out::spawn().unwrap(),
            None => {}
        };
        poll_input::spawn_after(Duration::millis(1)).unwrap();
    }

    #[task(binds = USBCTRL_IRQ, priority = 3, local = [serial, usb_dev])]
    fn usb_rx(cx: usb_rx::Context) {
        let usb_dev = cx.local.usb_dev;
        let serial = cx.local.serial;

        // Check for new data
        if usb_dev.poll(&mut [serial]) {
            let mut buf = [0u8; 64];
            match serial.read(&mut buf) {
                Err(_e) => {
                    // Do nothing
                }
                Ok(0) => {
                    // Do nothing
                }
                Ok(count) => {
                    buf.iter().take(count).for_each(|b| {
                        if b == &b'z' {
                            let _ = serial.write(b"Reboot\r\n");
                            reset_to_usb_boot(0, 0)
                        }
                    });
                }
            }
        }
    }
}

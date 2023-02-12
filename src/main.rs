#![no_std]
#![no_main]

use rtic::app;

use panic_halt as _;

#[app(device = rp_pico::hal::pac, dispatchers = [SW0_IRQ])]
mod app {

    use embedded_hal::digital::v2::OutputPin;
    use rp2040_monotonic::Rp2040Monotonic;
    use rp_pico::{
        hal::{
            clocks::init_clocks_and_plls,
            gpio::{Pin, PushPullOutput},
            rom_data::reset_to_usb_boot,
            usb::UsbBus,
            Sio, Watchdog,
        },
        Pins,
    };

    use usb_device::{
        class_prelude::UsbBusAllocator,
        device::{UsbDeviceBuilder, UsbVidPid},
    };
    use usbd_serial::SerialPort;

    const XTAL_FREQ_HZ: u32 = 12_000_000;

    #[monotonic(binds = TIMER_IRQ_0, default = true)]
    type MyMono = Rp2040Monotonic;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        led: Pin<rp_pico::hal::gpio::pin::bank0::Gpio25, PushPullOutput>,
        usb_dev: usb_device::device::UsbDevice<'static, UsbBus>,
        serial: SerialPort<'static, UsbBus>,
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

        let led = pins.led.into_push_pull_output();

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

        blink::spawn().unwrap();
        (
            Shared {},
            Local {
                led,
                usb_dev,
                serial,
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
        let d = rp2040_monotonic::fugit::TimerDurationU64::<1_000_000>::millis(500);
        blink::spawn_after(d).unwrap();
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

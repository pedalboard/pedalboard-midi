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
            usb::UsbBus,
            Sio, Watchdog,
        },
        Pins,
    };

    use usb_device::class_prelude::UsbBusAllocator;

    const XTAL_FREQ_HZ: u32 = 12_000_000;

    static mut USB_BUS: Option<usb_device::bus::UsbBusAllocator<UsbBus>> = None;

    #[monotonic(binds = TIMER_IRQ_0, default = true)]
    type MyMono = Rp2040Monotonic;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        led: Pin<rp_pico::hal::gpio::pin::bank0::Gpio25, PushPullOutput>,
    }

    #[init]
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

        // The bus that is used to manage the device and class below.
        let usb_bus = UsbBusAllocator::new(UsbBus::new(
            cx.device.USBCTRL_REGS,
            cx.device.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut resets,
        ));

        // We store the bus in a static to make the borrows satisfy the rtic model, since rtic
        // needs all references to be 'static.
        unsafe {
            USB_BUS = Some(usb_bus);
        }

        blink::spawn().unwrap();
        (Shared {}, Local { led }, init::Monotonics(mono))
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
}

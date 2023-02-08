#![no_std]
#![no_main]

use rtic::app;

use panic_halt as _;

#[app(device = rp_pico::hal::pac, dispatchers = [SW0_IRQ])]
mod app {

    use embedded_hal::digital::v2::OutputPin;
    use rp2040_monotonic::Rp2040Monotonic;
    use rp_pico::hal;

    #[monotonic(binds = TIMER_IRQ_0, default = true)]
    type MyMono = Rp2040Monotonic;

    #[shared]
    struct Shared {
        led: hal::gpio::Pin<hal::gpio::pin::bank0::Gpio25, hal::gpio::PushPullOutput>,
    }

    #[local]
    struct Local {}

    #[init]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        let sio = hal::Sio::new(cx.device.SIO);
        let mut resets = cx.device.RESETS;
        let timer = cx.device.TIMER;
        let mono = Rp2040Monotonic::new(timer);

        let pins = rp_pico::Pins::new(
            cx.device.IO_BANK0,
            cx.device.PADS_BANK0,
            sio.gpio_bank0,
            &mut resets,
        );

        let led = pins.led.into_push_pull_output();
        led_on::spawn().unwrap();
        (Shared { led }, Local {}, init::Monotonics(mono))
    }

    #[task(shared = [led])]
    fn led_on(cx: led_on::Context) {
        let mut led = cx.shared.led;
        (led).lock(|led_l| {
            led_l.set_high().unwrap();
        });

        let d = rp2040_monotonic::fugit::TimerDurationU64::<1_000_000>::secs(1);
        led_off::spawn_after(d).unwrap();
    }

    #[task(shared = [led])]
    fn led_off(cx: led_off::Context) {
        let mut led = cx.shared.led;
        (led).lock(|led_l| {
            led_l.set_low().unwrap();
        });
        let d = rp2040_monotonic::fugit::TimerDurationU64::<1_000_000>::secs(1);
        led_on::spawn_after(d).unwrap();
    }
}

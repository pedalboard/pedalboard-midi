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
    struct Shared {}

    #[local]
    struct Local {
        led: hal::gpio::Pin<hal::gpio::pin::bank0::Gpio25, hal::gpio::PushPullOutput>,
    }

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
        let d = rp2040_monotonic::fugit::TimerDurationU64::<1_000_000>::secs(1);
        blink::spawn_after(d).unwrap();
    }
}

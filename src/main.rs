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

        let led_pin = pins.led.into_push_pull_output();
        let d1 = rp2040_monotonic::fugit::Duration::<u64, 1, 1_000_000>::secs(1);
        led_on::spawn_after(d1).unwrap();
        (Shared { led: led_pin }, Local {}, init::Monotonics(mono))
    }

    #[task( shared = [led],)]
    fn led_on(cx: led_on::Context) {
        let mut led = cx.shared.led;
        (led).lock(|led_l| {
            led_l.set_high().unwrap();
        });

        let d1 = rp2040_monotonic::fugit::Duration::<u64, 1, 1_000_000>::secs(1);
        led_off::spawn_after(d1).unwrap();
    }

    #[task( shared = [led],)]
    fn led_off(cx: led_off::Context) {
        let mut led = cx.shared.led;
        (led).lock(|led_l| {
            led_l.set_low().unwrap();
        });
        let d1 = rp2040_monotonic::fugit::Duration::<u64, 1, 1_000_000>::secs(1);
        led_on::spawn_after(d1).unwrap();
    }
}

/*
use bsp::entry;
use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::v2::OutputPin;
use panic_probe as _;

// Provide an alias for our BSP so we can switch targets quickly.
// Uncomment the BSP you included in Cargo.toml, the rest of the code does not need to change.
use rp_pico as bsp;
// use sparkfun_pro_micro_rp2040 as bsp;

use bsp::hal::{
    clocks::{init_clocks_and_plls, Clock},
    pac,
    sio::Sio,
    watchdog::Watchdog,
};

#[entry]
fn main() -> ! {
    info!("Program start");
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);

    // External high-speed crystal on the pico board is 12Mhz
    let external_xtal_freq_hz = 12_000_000u32;
    let clocks = init_clocks_and_plls(
        external_xtal_freq_hz,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());

    let pins = bsp::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let mut led_pin = pins.led.into_push_pull_output();

    loop {
        info!("on!");
        led_pin.set_high().unwrap();
        delay.delay_ms(500);
        info!("off!");
        led_pin.set_low().unwrap();
        delay.delay_ms(500);
    }
}

*/

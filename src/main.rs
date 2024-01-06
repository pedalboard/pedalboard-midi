//! # I²C Example
//!
//! This application demonstrates how to talk to I²C devices with an RP2040.
//!
//! It may need to be adapted to your particular board layout and/or pin assignment.
//!
//! See the `Cargo.toml` file for Copyright and license details.

#![no_std]
#![no_main]

// Ensure we halt the program on panic (if we don't mention this crate it won't
// be linked)
use defmt_rtt as _;
use panic_probe as _;
use sh1107::mode::GraphicsMode;
// Some traits we need

// Alias for our HAL crate
use rp2040_hal as hal;

// Some traits we need
use hal::fugit::RateExtU32;

// A shorter alias for the Peripheral Access Crate, which provides low-level
// register access and a gpio related types.
use hal::{
    gpio::{FunctionI2C, Pin, PullUp},
    pac,
};

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

/// The linker will place this boot block at the start of our program image. We
/// need this to help the ROM bootloader get our code up and running.
/// Note: This boot block is not necessary when using a rp-hal based BSP
/// as the BSPs already perform this step.
#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

/// External high-speed crystal on the Raspberry Pi Pico board is 12 MHz. Adjust
/// if your board has a different frequency
const XTAL_FREQ_HZ: u32 = 12_000_000u32;

/// Entry point to our bare-metal application.
///
/// The `#[rp2040_hal::entry]` macro ensures the Cortex-M start-up code calls this function
/// as soon as all global variables and the spinlock are initialised.
///
/// The function configures the RP2040 peripherals, then performs a single I²C
/// write to a fixed address.
#[rp2040_hal::entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();

    // Set up the watchdog driver - needed by the clock setup code
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);

    // Configure the clocks
    let clocks = hal::clocks::init_clocks_and_plls(
        XTAL_FREQ_HZ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    // The single-cycle I/O block controls our GPIO pins
    let sio = hal::Sio::new(pac.SIO);

    // Set the pins to their default state
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // Configure two pins as being I²C
    let sda_pin: Pin<_, FunctionI2C, PullUp> = pins.gpio24.reconfigure();
    let scl_pin: Pin<_, FunctionI2C, PullUp> = pins.gpio25.reconfigure();

    let i2c = hal::I2C::i2c0(
        pac.I2C0,
        sda_pin,
        scl_pin,
        400.kHz(),
        &mut pac.RESETS,
        &clocks.system_clock,
    );

    let mut disp: GraphicsMode<_> = sh1107::Builder::new()
        .with_size(sh1107::prelude::DisplaySize::Display128x128)
        .with_rotation(sh1107::displayrotation::DisplayRotation::Rotate180)
        .connect_i2c(i2c)
        .into();

    disp.init().unwrap();
    disp.flush().unwrap();

    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    // Create a text at position (20, 30) and draw it using the previously defined style
    Text::new("Pedalbaord", Point::new(66, 30), style)
        .draw(&mut disp)
        .unwrap();
    Text::new("started", Point::new(66, 40), style)
        .draw(&mut disp)
        .unwrap();

    disp.flush().unwrap();

    /*
        // Scan for devices on the bus by attempting to read from them
        use embedded_hal::prelude::_embedded_hal_blocking_i2c_Read;
        for i in 0..=127 {
            defmt::info!("triying address {:?}", i);
            let mut readbuf: [u8; 1] = [0; 1];
            let result = i2c.read(i, &mut readbuf);
            if let Ok(_) = result {
                // Do whatever work you want to do with found devices
                defmt::info!("Device found at address {:?}", i);
            }
        }
    */
    // Write three bytes to the I²C device with 7-bit address 0x2C
    //   i2c.write(0x3c, &[1, 2, 3]).unwrap();

    // Demo finish - just loop until reset
    defmt::info!("Display should show Hello Rust now");
    loop {
        cortex_m::asm::wfi();
    }
}

// End of file

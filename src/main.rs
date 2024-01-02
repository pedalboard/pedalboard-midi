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
// Some traits we need

// Alias for our HAL crate
use rp2040_hal as hal;

// Some traits we need
use embedded_hal::blocking::i2c::Write;
use hal::fugit::RateExtU32;

// A shorter alias for the Peripheral Access Crate, which provides low-level
// register access and a gpio related types.
use hal::{
    gpio::{FunctionI2C, Pin, PinState, PullUp},
    pac,
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

    defmt::info!("starting");
    // Configure two pins as being I²C, not GPIO
    let sda_pin: Pin<_, FunctionI2C, PullUp> = pins.gpio24.reconfigure();
    let scl_pin: Pin<_, FunctionI2C, PullUp> = pins.gpio25.reconfigure();

    /*
        // Create the I²C drive, using the two pre-configured pins. This will fail
        // at compile time if the pins are in the wrong mode, or if this I²C
        // peripheral isn't available on these pins!
        let mut i2c = hal::I2C::i2c0(
            pac.I2C0,
            sda_pin,
            scl_pin, // Try `not_an_scl_pin` here
            400.kHz(),
            &mut pac.RESETS,
            &clocks.system_clock,
        );

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
        // Write three bytes to the I²C device with 7-bit address 0x2C
        //    i2c.write(0x2c, &[1, 2, 3]).unwrap();

        // Demo finish - just loop until reset

    */

    defmt::info!("Device not found");
    loop {
        cortex_m::asm::wfi();
    }
}

// End of file

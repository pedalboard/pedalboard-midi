use rp2040_hal::{
    gpio::{
        bank0::{Gpio24, Gpio25},
        FunctionI2C, Pin, PullUp,
    },
    i2c::I2C,
    pac::I2C0,
};

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

use sh1107 as driver;

pub type Display = driver::mode::GraphicsMode<
    driver::interface::I2cInterface<
        I2C<
            I2C0,
            (
                Pin<Gpio24, FunctionI2C, PullUp>,
                Pin<Gpio25, FunctionI2C, PullUp>,
            ),
        >,
    >,
>;

pub fn splash_screen(disp: &mut Display) {
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    // Create a text at position (20, 30) and draw it using the previously defined style
    Text::new("Pedalbaord MIDI", Point::new(0, 10), style)
        .draw(disp)
        .unwrap();
    Text::new("started", Point::new(0, 127), style)
        .draw(disp)
        .unwrap();

    disp.flush().unwrap();
}

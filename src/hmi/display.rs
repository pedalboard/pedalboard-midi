use rp2040_hal::{
    gpio::{
        bank0::{Gpio24, Gpio25},
        FunctionI2C, Pin, PullUp,
    },
    i2c::I2C,
    pac::I2C0,
};
use tinybmp::Bmp;

use embedded_graphics::{
    image::Image,
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

use sh1107 as driver;

pub type Interface = I2C<
    I2C0,
    (
        Pin<Gpio24, FunctionI2C, PullUp>,
        Pin<Gpio25, FunctionI2C, PullUp>,
    ),
>;

pub type Driver = driver::mode::GraphicsMode<driver::interface::I2cInterface<Interface>>;

pub struct Display {
    driver: Option<Driver>,
}

impl Display {
    pub fn new(i2c: Interface) -> Self {
        let mut driver: sh1107::mode::GraphicsMode<_> = sh1107::Builder::new()
            .with_size(sh1107::prelude::DisplaySize::Display128x128)
            .with_rotation(sh1107::displayrotation::DisplayRotation::Rotate180)
            .connect_i2c(i2c)
            .into();

        match driver.init() {
            Err(_) => Display {
                driver: Option::None,
            },
            Ok(_) => Display {
                driver: Option::Some(driver),
            },
        }
    }
    pub fn splash_screen(&mut self) {
        if let Some(disp) = &mut self.driver {
            let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
            let bmp_data = include_bytes!("../../img/pedalboard-logo.bmp");

            let bmp = Bmp::from_slice(bmp_data).unwrap();

            Image::new(&bmp, Point::new(0, 0)).draw(disp).unwrap();

            Text::new("Pedalbaord   Platform", Point::new(0, 10), style)
                .draw(disp)
                .unwrap();

            disp.flush().unwrap();
        }
    }
}

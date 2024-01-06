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

pub type Driver = driver::mode::GraphicsMode<
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

pub struct Display {
    driver: Driver,
}

impl Display {
    pub fn new(driver: Driver) -> Self {
        Display { driver }
    }
    pub fn splash_screen(&mut self) {
        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        let bmp_data = include_bytes!("../../img/pedalboard-logo.bmp");

        let bmp = Bmp::from_slice(bmp_data).unwrap();

        Image::new(&bmp, Point::new(0, 0))
            .draw(&mut self.driver)
            .unwrap();

        Text::new("Pedalbaord   Platform", Point::new(0, 10), style)
            .draw(&mut self.driver)
            .unwrap();

        self.driver.flush().unwrap();
    }
}

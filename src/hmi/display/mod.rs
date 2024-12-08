use rp2040_hal::{
    gpio::{
        bank0::{Gpio24, Gpio25},
        FunctionI2C, Pin, PullUp,
    },
    i2c::I2C,
    pac::I2C0,
};
use ssd1327_i2c::SSD1327I2C;
use tinybmp::Bmp;

use embedded_graphics::{
    image::Image,
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Gray4,
    prelude::*,
    primitives::Rectangle,
};

use embedded_text::{
    alignment::{HorizontalAlignment, VerticalAlignment},
    style::TextBoxStyleBuilder,
    TextBox,
};

pub type Interface = I2C<
    I2C0,
    (
        Pin<Gpio24, FunctionI2C, PullUp>,
        Pin<Gpio25, FunctionI2C, PullUp>,
    ),
>;

macro_rules! description {
    () => {
        "Open Pedalboard Platform"
    };
}
macro_rules! git_hash {
    () => {
        env!("GIT_HASH")
    };
}

macro_rules! version {
    () => {
        env!("CARGO_PKG_VERSION")
    };
}

macro_rules! version_string {
    () => {
        concat!(description!(), " v", version!(), " (", git_hash!(), ")")
    };
}
pub type Driver = SSD1327I2C<Interface>;

pub struct Display {
    driver: Option<Driver>,
}

impl Display {
    pub fn new(i2c: Interface) -> Self {
        let mut driver = ssd1327_i2c::SSD1327I2C::with_addr(i2c, 0x3D);
        driver.init();

        Display {
            driver: Option::Some(driver),
        }
    }
    pub fn splash_screen(&mut self) {
        if let Some(disp) = &mut self.driver {
            let bmp_data = include_bytes!("../../../img/pedalboard-logo.bmp");

            let bmp = Bmp::from_slice(bmp_data).unwrap();

            Image::new(&bmp, Point::new(0, 0)).draw(disp).unwrap();

            disp.flush().unwrap();
        }
    }

    pub fn show(&mut self) {
        if let Some(display) = &mut self.driver {
            display.clear(Gray4::BLACK).unwrap();

            let text = version_string!();
            let character_style = MonoTextStyle::new(&FONT_10X20, Gray4::WHITE);

            let textbox_style = TextBoxStyleBuilder::new()
                .alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Middle)
                .paragraph_spacing(6)
                .build();

            let bounds = Rectangle::new(Point::zero(), Size::new(128, 128));
            let text_box =
                TextBox::with_textbox_style(text, bounds, character_style, textbox_style);

            text_box.draw(display).unwrap();

            display.flush().unwrap();
        }
    }
}

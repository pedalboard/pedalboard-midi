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
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::Rectangle,
};

use embedded_text::{
    alignment::HorizontalAlignment,
    style::{HeightMode, TextBoxStyleBuilder},
    TextBox,
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
            let bmp_data = include_bytes!("../../img/pedalboard-logo.bmp");

            let bmp = Bmp::from_slice(bmp_data).unwrap();

            Image::new(&bmp, Point::new(0, 0)).draw(disp).unwrap();

            disp.flush().unwrap();
        }
    }

    pub fn show(&mut self) {
        if let Some(display) = &mut self.driver {
            display.clear();

            let text = "Open Pedalboard Platform";

            let character_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);

            let textbox_style = TextBoxStyleBuilder::new()
                .height_mode(HeightMode::FitToText)
                .alignment(HorizontalAlignment::Center)
                .paragraph_spacing(6)
                .build();

            let bounds = Rectangle::new(Point::zero(), Size::new(128, 0));
            let text_box =
                TextBox::with_textbox_style(text, bounds, character_style, textbox_style);

            text_box.draw(display).unwrap();

            display.flush().unwrap();
        }
    }
}

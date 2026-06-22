use core::fmt::Write;
use embedded_graphics::{
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
use heapless::String;

const DISPLAY_SIZE: u32 = 128;

/// Draw a centered overlay: label on top half, large value on bottom half
pub fn draw<D: DrawTarget<Color = Gray4>>(
    display: &mut D,
    label: &str,
    value: u8,
) -> Result<(), D::Error> {
    let style = MonoTextStyle::new(&FONT_10X20, Gray4::WHITE);
    let centered = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Middle)
        .build();

    // Label in top half
    let top = Rectangle::new(Point::zero(), Size::new(DISPLAY_SIZE, DISPLAY_SIZE / 2));
    TextBox::with_textbox_style(label, top, style, centered).draw(display)?;

    // Value in bottom half
    let mut buf: String<4> = String::new();
    write!(buf, "{}", value).ok();
    let bottom = Rectangle::new(
        Point::new(0, (DISPLAY_SIZE / 2) as i32),
        Size::new(DISPLAY_SIZE, DISPLAY_SIZE / 2),
    );
    TextBox::with_textbox_style(buf.as_str(), bottom, style, centered).draw(display)?;

    Ok(())
}

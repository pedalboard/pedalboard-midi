use core::fmt::Write;
use eg_seven_segment::SevenSegmentStyleBuilder;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Gray4,
    prelude::*,
    primitives::Rectangle,
    text::Text,
};
use heapless::String;

const DISPLAY_SIZE: u32 = 128;

/// Draw a large preset number (7-segment style) with the preset name below
pub fn draw<D: DrawTarget<Color = Gray4>>(
    display: &mut D,
    number: u8,
    name: &str,
) -> Result<(), D::Error> {
    // Large 7-segment number in upper 2/3
    let seg_style = SevenSegmentStyleBuilder::new()
        .digit_size(Size::new(40, 70))
        .segment_width(8)
        .segment_color(Gray4::WHITE)
        .build();

    let mut buf: String<4> = String::new();
    write!(buf, "{}", number).ok();

    // Center the number horizontally
    let digit_count = buf.len() as u32;
    let total_width = digit_count * 50; // ~50px per digit with spacing
    let x = ((DISPLAY_SIZE - total_width) / 2) as i32;

    Text::new(buf.as_str(), Point::new(x, 75), seg_style)
        .draw(display)?;

    // Preset name below in normal font
    let text_style = MonoTextStyle::new(&FONT_10X20, Gray4::WHITE);
    let name_width = (name.len() as u32) * 10;
    let name_x = ((DISPLAY_SIZE - name_width) / 2) as i32;
    Text::new(name, Point::new(name_x.max(0), 110), text_style)
        .draw(display)?;

    Ok(())
}

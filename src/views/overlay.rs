use core::fmt::Write;
use eg_seven_segment::SevenSegmentStyleBuilder;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Gray4,
    prelude::*,
    primitives::Rectangle,
    text::Text,
};
use embedded_text::{
    alignment::{HorizontalAlignment, VerticalAlignment},
    style::TextBoxStyleBuilder,
    TextBox,
};
use heapless::String;

const DISPLAY_SIZE: u32 = 128;

/// Draw a centered overlay: label on top, large 7-segment value below
pub fn draw<D: DrawTarget<Color = Gray4>>(
    display: &mut D,
    label: &str,
    value: u8,
) -> Result<(), D::Error> {
    // Label in top portion
    let text_style = MonoTextStyle::new(&FONT_10X20, Gray4::WHITE);
    let centered = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Middle)
        .build();
    let top = Rectangle::new(Point::zero(), Size::new(DISPLAY_SIZE, 40));
    TextBox::with_textbox_style(label, top, text_style, centered).draw(display)?;

    // Large 7-segment value
    let seg_style = SevenSegmentStyleBuilder::new()
        .digit_size(Size::new(30, 55))
        .segment_width(6)
        .segment_color(Gray4::WHITE)
        .build();

    let mut buf: String<4> = String::new();
    write!(buf, "{}", value).ok();

    let digit_count = buf.len() as u32;
    let total_width = digit_count * 40;
    let x = ((DISPLAY_SIZE - total_width) / 2) as i32;

    Text::new(buf.as_str(), Point::new(x, 100), seg_style).draw(display)?;

    // Intensity bar at bottom
    let bar_y = 110i32;
    let bar_h = 10u32;
    let bar_margin = 10u32;
    let bar_max_w = DISPLAY_SIZE - 2 * bar_margin;
    let bar_w = (value as u32 * bar_max_w) / 127;

    use embedded_graphics::primitives::CornerRadii;
    use embedded_graphics::primitives::{PrimitiveStyleBuilder, RoundedRectangle};

    // Background
    let bg = PrimitiveStyleBuilder::new()
        .fill_color(Gray4::new(0x3))
        .build();
    RoundedRectangle::new(
        Rectangle::new(
            Point::new(bar_margin as i32, bar_y),
            Size::new(bar_max_w, bar_h),
        ),
        CornerRadii::new(Size::new(4, 4)),
    )
    .into_styled(bg)
    .draw(display)?;

    // Fill
    if bar_w > 0 {
        let fg = PrimitiveStyleBuilder::new()
            .fill_color(Gray4::WHITE)
            .build();
        RoundedRectangle::new(
            Rectangle::new(
                Point::new(bar_margin as i32, bar_y),
                Size::new(bar_w, bar_h),
            ),
            CornerRadii::new(Size::new(4, 4)),
        )
        .into_styled(fg)
        .draw(display)?;
    }

    Ok(())
}

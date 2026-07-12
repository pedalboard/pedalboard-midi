//! Config mode display views — renders diagnostic information on the OLEDs.

use core::fmt::Write;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, ascii::FONT_6X9, MonoTextStyle},
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

use crate::config_mode::InfoScreen;

const DISPLAY_SIZE: u32 = 128;

/// Draw the config mode entry banner on a display.
pub fn draw_entered<D: DrawTarget<Color = Gray4>>(display: &mut D) -> Result<(), D::Error> {
    let style = MonoTextStyle::new(&FONT_10X20, Gray4::WHITE);
    let textbox_style = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Middle)
        .build();
    let bounds = Rectangle::new(Point::zero(), Size::new(DISPLAY_SIZE, DISPLAY_SIZE));
    TextBox::with_textbox_style("CONFIG\nMODE", bounds, style, textbox_style).draw(display)?;
    Ok(())
}

/// Draw the info/idle screen (left display: version + presets, right display: config summary).
pub fn draw_info_left<D: DrawTarget<Color = Gray4>>(
    display: &mut D,
    info: &InfoScreen,
) -> Result<(), D::Error> {
    let style = MonoTextStyle::new(&FONT_6X9, Gray4::WHITE);
    let textbox_style = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Left)
        .build();

    let mut buf: String<128> = String::new();
    writeln!(buf, "Firmware:").ok();
    writeln!(buf, "  {}", info.firmware_version).ok();
    writeln!(buf, "  {}", info.git_hash).ok();
    writeln!(buf).ok();
    writeln!(buf, "Presets: {}", info.preset_count).ok();
    writeln!(buf).ok();
    writeln!(buf, "---").ok();
    writeln!(buf, "Hold Vol+Gain").ok();
    writeln!(buf, "to exit").ok();

    let bounds = Rectangle::new(
        Point::new(4, 4),
        Size::new(DISPLAY_SIZE - 8, DISPLAY_SIZE - 8),
    );
    TextBox::with_textbox_style(buf.as_str(), bounds, style, textbox_style).draw(display)?;
    Ok(())
}

pub fn draw_info_right<D: DrawTarget<Color = Gray4>>(
    display: &mut D,
    info: &InfoScreen,
) -> Result<(), D::Error> {
    let style = MonoTextStyle::new(&FONT_6X9, Gray4::WHITE);
    let textbox_style = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Left)
        .build();

    let mut buf: String<128> = String::new();
    writeln!(buf, "Config:").ok();
    writeln!(
        buf,
        "  DIN out: {}",
        if info.din_enabled { "on" } else { "off" }
    )
    .ok();
    writeln!(
        buf,
        "  Clock: {}",
        if info.midi_clock { "on" } else { "off" }
    )
    .ok();
    writeln!(buf, "  BPM: {}", info.bpm).ok();
    writeln!(buf).ok();
    writeln!(buf, "Routing:").ok();
    writeln!(
        buf,
        "  DIN>USB: {}",
        if info.din_to_usb_thru { "on" } else { "off" }
    )
    .ok();
    writeln!(
        buf,
        "  USB>DIN: {}",
        if info.usb_to_din_thru { "on" } else { "off" }
    )
    .ok();
    writeln!(
        buf,
        "  USB>USB: {}",
        if info.usb_to_usb_thru { "on" } else { "off" }
    )
    .ok();

    let bounds = Rectangle::new(
        Point::new(4, 4),
        Size::new(DISPLAY_SIZE - 8, DISPLAY_SIZE - 8),
    );
    TextBox::with_textbox_style(buf.as_str(), bounds, style, textbox_style).draw(display)?;
    Ok(())
}

/// Draw button press feedback (centered, large button name + action detail).
pub fn draw_button_press<D: DrawTarget<Color = Gray4>>(
    display: &mut D,
    button: &str,
    detail: &str,
) -> Result<(), D::Error> {
    let title_style = MonoTextStyle::new(&FONT_10X20, Gray4::WHITE);
    let detail_style = MonoTextStyle::new(&FONT_6X9, Gray4::WHITE);

    let title_box = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Middle)
        .build();

    // Button name in top half.
    let top = Rectangle::new(Point::zero(), Size::new(DISPLAY_SIZE, 64));
    let mut title: String<8> = String::new();
    write!(title, "Btn {}", button).ok();
    TextBox::with_textbox_style(title.as_str(), top, title_style, title_box).draw(display)?;

    // Detail in bottom half.
    let detail_box = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Middle)
        .build();
    let bottom = Rectangle::new(Point::new(0, 64), Size::new(DISPLAY_SIZE, 64));
    TextBox::with_textbox_style(detail, bottom, detail_style, detail_box).draw(display)?;
    Ok(())
}

/// Draw encoder turn feedback.
pub fn draw_encoder_turn<D: DrawTarget<Color = Gray4>>(
    display: &mut D,
    encoder: &str,
    detail: &str,
) -> Result<(), D::Error> {
    let title_style = MonoTextStyle::new(&FONT_10X20, Gray4::WHITE);
    let detail_style = MonoTextStyle::new(&FONT_6X9, Gray4::WHITE);

    let title_box = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Middle)
        .build();

    // Encoder name in top half.
    let top = Rectangle::new(Point::zero(), Size::new(DISPLAY_SIZE, 64));
    TextBox::with_textbox_style(encoder, top, title_style, title_box).draw(display)?;

    // MIDI detail in bottom half.
    let detail_box = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Middle)
        .build();
    let bottom = Rectangle::new(Point::new(0, 64), Size::new(DISPLAY_SIZE, 64));
    TextBox::with_textbox_style(detail, bottom, detail_style, detail_box).draw(display)?;
    Ok(())
}

/// Draw expression pedal raw ADC value (large number for calibration) + MIDI detail.
pub fn draw_expression<D: DrawTarget<Color = Gray4>>(
    display: &mut D,
    pedal: &str,
    raw_adc: u16,
    detail: &str,
) -> Result<(), D::Error> {
    let title_style = MonoTextStyle::new(&FONT_10X20, Gray4::WHITE);
    let detail_style = MonoTextStyle::new(&FONT_6X9, Gray4::WHITE);

    // Pedal name at top.
    let title_box = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Middle)
        .build();
    let top = Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_SIZE, 30));
    TextBox::with_textbox_style(pedal, top, title_style, title_box).draw(display)?;

    // Large ADC value in center.
    let mut buf: String<8> = String::new();
    write!(buf, "{}", raw_adc).ok();
    let mid = Rectangle::new(Point::new(0, 30), Size::new(DISPLAY_SIZE, 40));
    TextBox::with_textbox_style(buf.as_str(), mid, title_style, title_box).draw(display)?;

    // MIDI detail below.
    if !detail.is_empty() {
        let detail_box = TextBoxStyleBuilder::new()
            .alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Middle)
            .build();
        let detail_area = Rectangle::new(Point::new(0, 72), Size::new(DISPLAY_SIZE, 24));
        TextBox::with_textbox_style(detail, detail_area, detail_style, detail_box).draw(display)?;
    }

    // Draw a proportional bar at bottom (0-4095 range).
    let bar_y = 108i32;
    let bar_h = 12u32;
    let bar_margin = 8u32;
    let bar_max_w = DISPLAY_SIZE - 2 * bar_margin;
    let bar_w = (raw_adc as u32 * bar_max_w) / 4095;

    use embedded_graphics::primitives::CornerRadii;
    use embedded_graphics::primitives::{PrimitiveStyleBuilder, RoundedRectangle};

    // Background.
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

    // Fill.
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

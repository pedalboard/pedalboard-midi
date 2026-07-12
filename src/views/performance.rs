use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Gray4,
    prelude::*,
    primitives::{
        CornerRadiiBuilder, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
        Triangle,
    },
};
use embedded_text::{
    alignment::{HorizontalAlignment, VerticalAlignment},
    style::TextBoxStyleBuilder,
    TextBox,
};
use heapless::String;

const DISPLAY_SIZE: u32 = 128;
const PADDING: u32 = 3;
const ROWS: u32 = 3;
const ROW_HEIGHT: u32 = (DISPLAY_SIZE - ((ROWS + 1) * PADDING)) / ROWS;
const ROW_WIDTH: u32 = DISPLAY_SIZE - 2 * PADDING;
const CORNER_RADIUS: u32 = 14;

/// Row start Y positions (pixels) for each of the 3 button slots.
pub const ROW_Y: [u32; 3] = [
    PADDING,
    PADDING + ROW_HEIGHT + PADDING,
    PADDING + 2 * (ROW_HEIGHT + PADDING),
];

/// Flush region for a given row index (start_row, end_row inclusive).
/// Includes 1px margin above and below to cover anti-aliased edges.
pub fn row_flush_range(row_idx: usize) -> (u8, u8) {
    let start = if ROW_Y[row_idx] > 0 {
        ROW_Y[row_idx] - 1
    } else {
        0
    };
    let end = (ROW_Y[row_idx] + ROW_HEIGHT + 1).min(DISPLAY_SIZE - 1);
    (start as u8, end as u8)
}

/// Flush region that covers the preset number indicator (left display, bottom-right).
/// Returns (start_row, end_row) inclusive.
pub fn preset_number_flush_range() -> (u8, u8) {
    // The 7-segment digits are 16px tall, drawn at y = DISPLAY_SIZE - 4 (baseline).
    // Actual top of digit is around y = DISPLAY_SIZE - 4 - 16 = 108.
    (107, 127)
}

pub const BUTTON_COUNT: usize = 6;

#[derive(Debug, Clone)]
pub struct PresetMeta {
    pub name: String<16>,
    pub preset_number: u8,
    pub button_labels: [String<16>; BUTTON_COUNT],
    pub button_active: [bool; BUTTON_COUNT],
    /// Short hint for long-press action (e.g., "» Next", "« Prev"), empty if none.
    pub long_press_hints: [String<8>; BUTTON_COUNT],
}

impl Default for PresetMeta {
    fn default() -> Self {
        Self {
            name: String::new(),
            preset_number: 0,
            button_labels: core::array::from_fn(|_| String::new()),
            button_active: [false; BUTTON_COUNT],
            long_press_hints: core::array::from_fn(|_| String::new()),
        }
    }
}

/// Which display to render
#[derive(Debug, Clone, Copy)]
pub enum Side {
    /// Left display: D (top), E (middle), A (bottom)
    Left,
    /// Right display: F (top), B (middle), C (bottom)
    Right,
}

/// Draw button labels in rounded rectangles with arrow corners.
/// Each label is positioned at a fixed vertical slot corresponding to its
/// physical button location (top/mid/bottom). Empty labels leave their
/// slot blank — remaining labels do NOT shift or resize.
///
/// Physical layout:
///   D      E      F
///      [L]    [R]
///   A      B      C
///
/// Left display shows:  D (top-left↗), E (mid-left→), A (bottom-left↘)
/// Right display shows: F (top-right↖), B (mid-right←), C (bottom-right↙)
pub fn draw<D: DrawTarget<Color = Gray4>>(
    display: &mut D,
    preset: &PresetMeta,
    side: Side,
) -> Result<(), D::Error> {
    for row in 0..ROWS {
        draw_single_row(display, preset, side, row)?;
    }

    // Preset number indicator (left display only, bottom-right corner, 7-segment style)
    if matches!(side, Side::Left) {
        draw_preset_number(display, preset.preset_number)?;
    }

    Ok(())
}

/// Draw a single button row (0=top, 1=mid, 2=bottom) for the given side.
/// Clears the row region before drawing. Use with `flush_rows` for partial updates.
pub fn draw_row<D: DrawTarget<Color = Gray4>>(
    display: &mut D,
    preset: &PresetMeta,
    side: Side,
    row: u32,
) -> Result<(), D::Error> {
    // Clear the row region in the framebuffer
    let y = ROW_Y[row as usize];
    let clear_rect = Rectangle::new(
        Point::new(0, y as i32),
        Size::new(DISPLAY_SIZE, ROW_HEIGHT + PADDING),
    );
    display.fill_solid(&clear_rect, Gray4::BLACK)?;

    draw_single_row(display, preset, side, row)
}

/// Draw the preset number indicator (left display only).
pub fn draw_preset_number<D: DrawTarget<Color = Gray4>>(
    display: &mut D,
    preset_number: u8,
) -> Result<(), D::Error> {
    use core::fmt::Write;
    use eg_seven_segment::SevenSegmentStyleBuilder;
    use embedded_graphics::text::Text;

    let seg_style = SevenSegmentStyleBuilder::new()
        .digit_size(Size::new(9, 16))
        .segment_width(2)
        .segment_color(Gray4::WHITE)
        .build();
    let mut buf: String<4> = String::new();
    write!(buf, "{}", preset_number).ok();

    let digit_count = buf.len() as i32;
    let digit_width = 13;
    let x = DISPLAY_SIZE as i32 - 4 - digit_count * digit_width;
    let y = DISPLAY_SIZE as i32 - 4;

    Text::new(buf.as_str(), Point::new(x, y), seg_style).draw(display)?;
    Ok(())
}

/// Map button index to (side, row_index) — returns which display side and row slot
/// the button occupies.
pub fn button_to_row(btn_idx: usize) -> (Side, u32) {
    match btn_idx {
        3 => (Side::Left, 0),  // D = left top
        4 => (Side::Left, 1),  // E = left mid
        0 => (Side::Left, 2),  // A = left bottom
        5 => (Side::Right, 0), // F = right top
        1 => (Side::Right, 1), // B = right mid
        2 => (Side::Right, 2), // C = right bottom
        _ => (Side::Left, 0),  // fallback
    }
}

/// Internal: render one button row without clearing.
fn draw_single_row<D: DrawTarget<Color = Gray4>>(
    display: &mut D,
    preset: &PresetMeta,
    side: Side,
    i: u32,
) -> Result<(), D::Error> {
    let indices = match side {
        Side::Left => [3, 4, 0],  // D, E, A
        Side::Right => [5, 1, 2], // F, B, C
    };

    let btn_idx = indices[i as usize];
    let label = &preset.button_labels[btn_idx];
    if label.is_empty() {
        return Ok(());
    }

    let stroke = PrimitiveStyle::with_stroke(Gray4::WHITE, 2);
    let text_style = MonoTextStyle::new(&FONT_10X20, Gray4::WHITE);
    let textbox_style = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Middle)
        .build();

    let active_fill = PrimitiveStyleBuilder::new()
        .fill_color(Gray4::WHITE)
        .build();
    let active_text_style = MonoTextStyle::new(&FONT_10X20, Gray4::BLACK);

    let radius = Size::new(CORNER_RADIUS, CORNER_RADIUS);
    const INSET: u32 = 30;

    let y = (PADDING + i * (ROW_HEIGHT + PADDING)) as i32;

    let sharp_on_left = matches!(
        (side, i),
        (Side::Left, 0) | (Side::Left, 2) | (Side::Right, 1)
    );

    let (x, w) = if sharp_on_left {
        (PADDING as i32, ROW_WIDTH - INSET)
    } else {
        ((PADDING + INSET) as i32, ROW_WIDTH - INSET)
    };

    let rect = Rectangle::new(Point::new(x, y), Size::new(w, ROW_HEIGHT));

    let radii = match (side, i) {
        (Side::Left, 0) => CornerRadiiBuilder::new()
            .all(radius)
            .top_left(Size::zero())
            .build(),
        (Side::Left, 1) => CornerRadiiBuilder::new()
            .all(radius)
            .top_right(Size::zero())
            .build(),
        (Side::Left, _) => CornerRadiiBuilder::new()
            .all(radius)
            .bottom_left(Size::zero())
            .build(),
        (Side::Right, 0) => CornerRadiiBuilder::new()
            .all(radius)
            .top_right(Size::zero())
            .build(),
        (Side::Right, 1) => CornerRadiiBuilder::new()
            .all(radius)
            .bottom_left(Size::zero())
            .build(),
        (Side::Right, _) => CornerRadiiBuilder::new()
            .all(radius)
            .bottom_right(Size::zero())
            .build(),
    };

    let is_active = preset.button_active[btn_idx];

    if is_active {
        RoundedRectangle::new(rect, radii)
            .into_styled(active_fill)
            .draw(display)?;
    } else {
        RoundedRectangle::new(rect, radii)
            .into_styled(stroke)
            .draw(display)?;
    }

    // Corner triangle indicator
    let cs = if is_active { 10i32 } else { 12i32 };
    let corner_style = if is_active {
        // Active: outline triangle (white stroke, black inside) for contrast on white fill
        PrimitiveStyleBuilder::new()
            .fill_color(Gray4::BLACK)
            .stroke_color(Gray4::WHITE)
            .stroke_width(2)
            .build()
    } else {
        // Inactive: solid white triangle on black background
        PrimitiveStyleBuilder::new()
            .fill_color(Gray4::WHITE)
            .build()
    };
    let corner_pos = match (side, i) {
        (Side::Left, 0) => {
            let o = if is_active { 2 } else { 0 };
            let p = rect.top_left + Point::new(o, o);
            Triangle::new(p, p + Point::new(cs, 0), p + Point::new(0, cs))
        }
        (Side::Left, 1) => {
            let o = if is_active { 2 } else { 0 };
            let p = rect.top_left + Point::new(w as i32 - 1 - o, o);
            Triangle::new(p, p + Point::new(-cs, 0), p + Point::new(0, cs))
        }
        (Side::Left, _) => {
            let o = if is_active { 2 } else { 0 };
            let p = rect.top_left + Point::new(o, ROW_HEIGHT as i32 - 1 - o);
            Triangle::new(p, p + Point::new(cs, 0), p + Point::new(0, -cs))
        }
        (Side::Right, 0) => {
            let o = if is_active { 2 } else { 0 };
            let p = rect.top_left + Point::new(w as i32 - 1 - o, o);
            Triangle::new(p, p + Point::new(-cs, 0), p + Point::new(0, cs))
        }
        (Side::Right, 1) => {
            let o = if is_active { 2 } else { 0 };
            let p = rect.top_left + Point::new(o, ROW_HEIGHT as i32 - 1 - o);
            Triangle::new(p, p + Point::new(cs, 0), p + Point::new(0, -cs))
        }
        (Side::Right, _) => {
            let o = if is_active { 2 } else { 0 };
            let p = rect.top_left + Point::new(w as i32 - 1 - o, ROW_HEIGHT as i32 - 1 - o);
            Triangle::new(p, p + Point::new(-cs, 0), p + Point::new(0, -cs))
        }
    };
    corner_pos.into_styled(corner_style).draw(display)?;

    // Label text
    if is_active {
        TextBox::with_textbox_style(label.as_str(), rect, active_text_style, textbox_style)
            .draw(display)?;
    } else {
        let shadow_style = MonoTextStyle::new(&FONT_10X20, Gray4::new(0x7));
        let shadow_rect = Rectangle::new(rect.top_left + Point::new(1, 1), rect.size);
        TextBox::with_textbox_style(label.as_str(), shadow_rect, shadow_style, textbox_style)
            .draw(display)?;
        TextBox::with_textbox_style(label.as_str(), rect, text_style, textbox_style)
            .draw(display)?;
    }

    // Long-press indicator (icon in the INSET gap of the button row)
    let hint = &preset.long_press_hints[btn_idx];
    if !hint.is_empty() {
        use embedded_graphics::primitives::{Circle, PrimitiveStyleBuilder, Triangle};

        let indicator_color = Gray4::WHITE;
        let indicator_style = PrimitiveStyleBuilder::new()
            .fill_color(indicator_color)
            .build();

        // Vertical center of this row (shift up for bottom-left to avoid preset number)
        let cy = if matches!(side, Side::Left) && i == 2 {
            y + (ROW_HEIGHT as i32) / 2 - 10
        } else {
            y + (ROW_HEIGHT as i32) / 2
        };
        const IND_SIZE: i32 = 14;

        let is_next = hint.as_str().contains("Next") || hint.as_str().contains("»");
        let is_prev = hint.as_str().contains("Prev") || hint.as_str().contains("«");

        // The INSET gap position
        let ix = if sharp_on_left {
            // Gap is on the right: from x + w to x + w + INSET
            x + w as i32 + (INSET as i32 - IND_SIZE) / 2
        } else {
            // Gap is on the left: from PADDING to PADDING + INSET
            PADDING as i32 + (INSET as i32 - IND_SIZE) / 2
        };

        if is_next {
            // Right-pointing triangle
            Triangle::new(
                Point::new(ix, cy - IND_SIZE / 2),
                Point::new(ix, cy + IND_SIZE / 2),
                Point::new(ix + IND_SIZE, cy),
            )
            .into_styled(indicator_style)
            .draw(display)?;
        } else if is_prev {
            // Left-pointing triangle
            Triangle::new(
                Point::new(ix + IND_SIZE, cy - IND_SIZE / 2),
                Point::new(ix + IND_SIZE, cy + IND_SIZE / 2),
                Point::new(ix, cy),
            )
            .into_styled(indicator_style)
            .draw(display)?;
        } else {
            // Dot (generic long-press action)
            Circle::new(Point::new(ix + 1, cy - 4), 8)
                .into_styled(indicator_style)
                .draw(display)?;
        }
    }

    Ok(())
}

/// Build display metadata (name + button labels) from a PE config preset.
/// Falls back to defaults ("Preset N", "A"-"F") if empty.
pub fn preset_meta_from_config(
    cfg: &midi_controller::config::Config,
    index: usize,
) -> (
    String<16>,
    [String<16>; BUTTON_COUNT],
    [String<8>; BUTTON_COUNT],
) {
    use midi_controller::config::Action;

    let defaults = ["A", "B", "C", "D", "E", "F"];
    if let Some(p) = cfg.presets.get(index) {
        let name = if p.name.is_empty() {
            let mut s: String<16> = String::new();
            core::fmt::Write::write_fmt(&mut s, format_args!("Preset {}", index + 1)).ok();
            s
        } else {
            p.name.clone()
        };
        let labels = core::array::from_fn(|j| {
            match p.buttons.get(j) {
                Some(b) => b.label.clone(), // empty label = intentionally hidden
                None => String::try_from(defaults[j]).unwrap_or_default(),
            }
        });
        let hints: [String<8>; BUTTON_COUNT] = core::array::from_fn(|j| {
            let mut hint: String<8> = String::new();
            if let Some(btn) = p.buttons.get(j) {
                if let Some(action) = btn.on_long_press.first() {
                    match action {
                        Action::PresetNext => {
                            core::fmt::Write::write_str(&mut hint, "» Next").ok();
                        }
                        Action::PresetPrev => {
                            core::fmt::Write::write_str(&mut hint, "« Prev").ok();
                        }
                        Action::PresetSelect(idx) => {
                            core::fmt::Write::write_fmt(&mut hint, format_args!("» {}", idx + 1))
                                .ok();
                        }
                        _ => {}
                    }
                }
            }
            hint
        });
        (name, labels, hints)
    } else {
        let mut name: String<16> = String::new();
        core::fmt::Write::write_fmt(&mut name, format_args!("Preset {}", index + 1)).ok();
        let labels = core::array::from_fn(|j| String::try_from(defaults[j]).unwrap_or_default());
        let hints = core::array::from_fn(|_| String::new());
        (name, labels, hints)
    }
}

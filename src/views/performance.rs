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
const ROW_WIDTH: u32 = DISPLAY_SIZE - 2 * PADDING;
const CORNER_RADIUS: u32 = 14;

pub const BUTTON_COUNT: usize = 6;

#[derive(Debug, Clone)]
pub struct PresetMeta {
    pub name: String<16>,
    pub button_labels: [String<16>; BUTTON_COUNT],
    pub button_active: [bool; BUTTON_COUNT],
}

impl Default for PresetMeta {
    fn default() -> Self {
        Self {
            name: String::new(),
            button_labels: core::array::from_fn(|_| String::new()),
            button_active: [false; BUTTON_COUNT],
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
/// Layout adapts dynamically based on how many buttons have non-empty labels:
/// - 3 labels: standard 3-row layout
/// - 2 labels: 2 taller rows filling the display
/// - 1 label: single large centered label
/// - 0 labels: nothing drawn
///
/// Buttons keep their physical position ordering (top/mid/bottom) so the
/// spatial relationship to the hardware buttons is preserved.
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
    // Button indices: A=0, B=1, C=2, D=3, E=4, F=5
    // Slot 0=top, 1=mid, 2=bottom
    let indices = match side {
        Side::Left => [3, 4, 0],  // D, E, A
        Side::Right => [5, 1, 2], // F, B, C
    };

    // Collect active (non-empty) slots with their original slot position
    let active_slots: heapless::Vec<u32, 3> = (0..3u32)
        .filter(|&i| !preset.button_labels[indices[i as usize]].is_empty())
        .collect();

    let num_active = active_slots.len() as u32;
    if num_active == 0 {
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
    const INSET: u32 = 8;

    // Compute row height dynamically
    let row_height = (DISPLAY_SIZE - ((num_active + 1) * PADDING)) / num_active;

    for (row_idx, &slot) in active_slots.iter().enumerate() {
        let btn_idx = indices[slot as usize];
        let label = &preset.button_labels[btn_idx];

        let y = (PADDING + row_idx as u32 * (row_height + PADDING)) as i32;

        let sharp_on_left = matches!(
            (side, slot),
            (Side::Left, 0) | (Side::Left, 2) | (Side::Right, 1)
        );

        let (x, w) = if sharp_on_left {
            (PADDING as i32, ROW_WIDTH - INSET)
        } else {
            ((PADDING + INSET) as i32, ROW_WIDTH - INSET)
        };

        let rect = Rectangle::new(Point::new(x, y), Size::new(w, row_height));

        // Sharp corner points toward the physical button location
        let radii = match (side, slot) {
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

        // Fill sharp corner with a solid triangle indicator
        let cs = 10i32;
        let corner_fill_color = if is_active {
            Gray4::BLACK
        } else {
            Gray4::WHITE
        };
        let fill = PrimitiveStyleBuilder::new()
            .fill_color(corner_fill_color)
            .build();
        let corner_pos = match (side, slot) {
            (Side::Left, 0) => Triangle::new(
                rect.top_left,
                rect.top_left + Point::new(cs, 0),
                rect.top_left + Point::new(0, cs),
            ),
            (Side::Left, 1) => {
                let p = rect.top_left + Point::new(w as i32 - 1, 0);
                Triangle::new(p, p + Point::new(-cs, 0), p + Point::new(0, cs))
            }
            (Side::Left, _) => {
                let p = rect.top_left + Point::new(0, row_height as i32 - 1);
                Triangle::new(p, p + Point::new(cs, 0), p + Point::new(0, -cs))
            }
            (Side::Right, 0) => {
                let p = rect.top_left + Point::new(w as i32 - 1, 0);
                Triangle::new(p, p + Point::new(-cs, 0), p + Point::new(0, cs))
            }
            (Side::Right, 1) => {
                let p = rect.top_left + Point::new(0, row_height as i32 - 1);
                Triangle::new(p, p + Point::new(cs, 0), p + Point::new(0, -cs))
            }
            (Side::Right, _) => {
                let p = rect.top_left + Point::new(w as i32 - 1, row_height as i32 - 1);
                Triangle::new(p, p + Point::new(-cs, 0), p + Point::new(0, -cs))
            }
        };
        corner_pos.into_styled(fill).draw(display)?;

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
    }

    Ok(())
}

/// Build display metadata (name + button labels) from a PE config preset.
/// Falls back to defaults ("Preset N", "A"-"F") if empty.
pub fn preset_meta_from_config(
    cfg: &pedalboard_protocol::config::Config,
    index: usize,
) -> (String<16>, [String<16>; BUTTON_COUNT]) {
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
        (name, labels)
    } else {
        let mut name: String<16> = String::new();
        core::fmt::Write::write_fmt(&mut name, format_args!("Preset {}", index + 1)).ok();
        let labels = core::array::from_fn(|j| String::try_from(defaults[j]).unwrap_or_default());
        (name, labels)
    }
}

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Gray4,
    prelude::*,
    primitives::{CornerRadiiBuilder, PrimitiveStyle, Rectangle, RoundedRectangle},
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

pub const BUTTON_COUNT: usize = 6;

#[derive(Debug, Clone)]
pub struct PresetMeta {
    pub name: String<16>,
    pub button_labels: [String<8>; BUTTON_COUNT],
}

impl Default for PresetMeta {
    fn default() -> Self {
        Self {
            name: String::new(),
            button_labels: core::array::from_fn(|_| String::new()),
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

/// Draw 3 button labels in rounded rectangles with arrow corners
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
    let indices = match side {
        Side::Left => [3, 4, 0],   // D, E, A
        Side::Right => [5, 1, 2],  // F, B, C
    };

    let stroke = PrimitiveStyle::with_stroke(Gray4::WHITE, 2);
    let text_style = MonoTextStyle::new(&FONT_10X20, Gray4::WHITE);
    let textbox_style = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Middle)
        .build();

    let radius = Size::new(CORNER_RADIUS, CORNER_RADIUS);

    for i in 0..ROWS {
        let y = (PADDING + i * (ROW_HEIGHT + PADDING)) as i32;
        let rect = Rectangle::new(
            Point::new(PADDING as i32, y),
            Size::new(ROW_WIDTH, ROW_HEIGHT),
        );

        // Sharp corner points toward the physical button location
        // D is above-left of L  → top-left
        // E is above-right of L → top-right
        // A is below-left of L  → bottom-left
        // F is above-right of R → top-right
        // B is below-left of R  → bottom-left
        // C is below-right of R → bottom-right
        let radii = match (side, i) {
            (Side::Left, 0) => CornerRadiiBuilder::new().all(radius).top_left(Size::zero()).build(),
            (Side::Left, 1) => CornerRadiiBuilder::new().all(radius).top_right(Size::zero()).build(),
            (Side::Left, _) => CornerRadiiBuilder::new().all(radius).bottom_left(Size::zero()).build(),
            (Side::Right, 0) => CornerRadiiBuilder::new().all(radius).top_right(Size::zero()).build(),
            (Side::Right, 1) => CornerRadiiBuilder::new().all(radius).bottom_left(Size::zero()).build(),
            (Side::Right, _) => CornerRadiiBuilder::new().all(radius).bottom_right(Size::zero()).build(),
        };

        RoundedRectangle::new(rect, radii)
            .into_styled(stroke)
            .draw(display)?;

        // Label text with shadow for depth, then white on top
        let label = &preset.button_labels[indices[i as usize]];
        let shadow_style = MonoTextStyle::new(&FONT_10X20, Gray4::new(0x7));
        let shadow_rect = Rectangle::new(
            rect.top_left + Point::new(1, 1),
            rect.size,
        );
        TextBox::with_textbox_style(label.as_str(), shadow_rect, shadow_style, textbox_style)
            .draw(display)?;
        TextBox::with_textbox_style(label.as_str(), rect, text_style, textbox_style)
            .draw(display)?;
    }

    Ok(())
}

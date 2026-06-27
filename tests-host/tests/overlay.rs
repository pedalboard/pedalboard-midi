// Host-side tests for src/views/overlay.rs

#[path = "../../src/views/overlay.rs"]
mod overlay;

use embedded_graphics::{
    geometry::Size, mock_display::MockDisplay, pixelcolor::Gray4, prelude::*,
    primitives::Rectangle,
};

fn make_display() -> MockDisplay<Gray4> {
    let mut d = MockDisplay::new();
    d.set_allow_out_of_bounds_drawing(true);
    d
}

#[test]
fn draw_encoder_overlay_renders_pixels() {
    let mut display = make_display();
    overlay::draw(&mut display, "Vol", 72).unwrap();
}

#[test]
fn draw_encoder_overlay_value_zero() {
    let mut display = make_display();
    overlay::draw(&mut display, "Gain", 0).unwrap();
}

#[test]
fn draw_encoder_overlay_value_max() {
    let mut display = make_display();
    overlay::draw(&mut display, "Vol", 127).unwrap();
}

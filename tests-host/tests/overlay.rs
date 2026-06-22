// Host-side tests for src/views/overlay.rs

#[path = "../../src/views/overlay.rs"]
mod overlay;

use embedded_graphics::{mock_display::MockDisplay, pixelcolor::Gray4};

#[test]
fn draw_encoder_overlay_renders_pixels() {
    let mut display = MockDisplay::<Gray4>::new();
    overlay::draw(&mut display, "Vol", 72).unwrap();
    assert!(!display.affected_area().is_zero_sized());
}

#[test]
fn draw_encoder_overlay_value_zero() {
    let mut display = MockDisplay::<Gray4>::new();
    overlay::draw(&mut display, "Gain", 0).unwrap();
    assert!(!display.affected_area().is_zero_sized());
}

#[test]
fn draw_encoder_overlay_value_max() {
    let mut display = MockDisplay::<Gray4>::new();
    overlay::draw(&mut display, "Vol", 127).unwrap();
    assert!(!display.affected_area().is_zero_sized());
}

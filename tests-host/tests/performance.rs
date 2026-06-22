// Host-side tests for src/views/performance.rs

#[path = "../../src/views/performance.rs"]
mod performance;

use embedded_graphics::{
    geometry::Size,
    mock_display::MockDisplay,
    pixelcolor::Gray4,
    prelude::*,
};
use heapless::String;
use performance::{PresetMeta, Side};

fn new_display() -> MockDisplay<Gray4> {
    let mut d = MockDisplay::new();
    d.set_allow_out_of_bounds_drawing(true);
    d.set_allow_overdraw(true);
    d
}

fn test_preset() -> PresetMeta {
    let mut p = PresetMeta::default();
    p.name = String::try_from("Clean+Delay").unwrap();
    p.button_labels[0] = String::try_from("Drive").unwrap();
    p.button_labels[1] = String::try_from("Delay").unwrap();
    p.button_labels[2] = String::try_from("Reverb").unwrap();
    p.button_labels[3] = String::try_from("Looper").unwrap();
    p.button_labels[4] = String::try_from("Tap").unwrap();
    p.button_labels[5] = String::try_from("Bank+").unwrap();
    p
}

#[test]
fn draw_left_renders_buttons_a_b_c() {
    let mut display = new_display();
    let preset = test_preset();
    performance::draw(&mut display, &preset, Side::Left).unwrap();
    assert!(!display.affected_area().is_zero_sized());
}

#[test]
fn draw_right_renders_buttons_d_e_f() {
    let mut display = new_display();
    let preset = test_preset();
    performance::draw(&mut display, &preset, Side::Right).unwrap();
    assert!(!display.affected_area().is_zero_sized());
}

#[test]
fn empty_labels_still_draws_borders() {
    let mut display = new_display();
    let preset = PresetMeta::default();
    performance::draw(&mut display, &preset, Side::Left).unwrap();
    // Rounded rectangles should still be drawn even with empty labels
    assert!(!display.affected_area().is_zero_sized());
}

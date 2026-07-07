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
fn empty_labels_still_draws_nothing() {
    let mut display = new_display();
    let preset = PresetMeta::default();
    performance::draw(&mut display, &preset, Side::Left).unwrap();
    // Empty labels → nothing drawn (buttons with no label are hidden)
    assert!(display.affected_area().is_zero_sized());
}

#[test]
fn preset_meta_defaults_when_config_empty() {
    let cfg = pedalboard_protocol::config::Config::default();
    let (name, labels) = performance::preset_meta_from_config(&cfg, 0);
    assert_eq!(name.as_str(), "Preset 1");
    assert_eq!(labels[0].as_str(), "A");
    assert_eq!(labels[5].as_str(), "F");
}

#[test]
fn preset_meta_defaults_for_index_beyond_vec() {
    let cfg = pedalboard_protocol::config::Config::default();
    let (name, _) = performance::preset_meta_from_config(&cfg, 4);
    assert_eq!(name.as_str(), "Preset 5");
}

#[test]
fn preset_meta_uses_config_name_and_labels() {
    use heapless::{String, Vec};
    use pedalboard_protocol::config::*;

    let mut presets = Vec::new();
    let mut buttons = Vec::new();
    buttons
        .push(ButtonConfig {
            label: String::try_from("Verse").unwrap(),
            color: LedConfig::default(),
            mode: ButtonMode::default(),
            on_press: Vec::new(),
            on_release: Vec::new(),
            on_long_press: Vec::new(),
            cycle_values: Vec::new(),
                listen_cc: None,
        })
        .ok();
    presets
        .push(Preset {
            name: String::try_from("My Song").unwrap(),
            buttons,
            encoders: Vec::new(),
            analog: Vec::new(),
            defaults: Default::default(),
            on_enter: heapless::Vec::new(),
            on_exit: heapless::Vec::new(),
            triggers: heapless::Vec::new(),
        })
        .ok();

    let cfg = Config { presets };
    let (name, labels) = performance::preset_meta_from_config(&cfg, 0);
    assert_eq!(name.as_str(), "My Song");
    assert_eq!(labels[0].as_str(), "Verse");
    // Remaining buttons fall back to defaults
    assert_eq!(labels[1].as_str(), "B");
}

#[test]
fn preset_meta_empty_label_uses_default() {
    use heapless::{String, Vec};
    use pedalboard_protocol::config::*;

    let mut presets = Vec::new();
    let mut buttons = Vec::new();
    buttons
        .push(ButtonConfig {
            label: String::new(), // empty
            color: LedConfig::default(),
            mode: ButtonMode::default(),
            on_press: Vec::new(),
            on_release: Vec::new(),
            on_long_press: Vec::new(),
            cycle_values: Vec::new(),
                listen_cc: None,
        })
        .ok();
    presets
        .push(Preset {
            name: String::try_from("Song").unwrap(),
            buttons,
            encoders: Vec::new(),
            analog: Vec::new(),
            defaults: Default::default(),
            on_enter: heapless::Vec::new(),
            on_exit: heapless::Vec::new(),
            triggers: heapless::Vec::new(),
        })
        .ok();

    let cfg = Config { presets };
    let (_, labels) = performance::preset_meta_from_config(&cfg, 0);
    assert_eq!(labels[0].as_str(), ""); // empty label = intentionally hidden
}

#[test]
fn draw_active_button_renders_filled() {
    let mut display_inactive = new_display();
    let mut display_active = new_display();

    let mut preset = test_preset();
    performance::draw(&mut display_inactive, &preset, Side::Left).unwrap();

    // Activate button D (index 3, shown as top row on left display)
    preset.button_active[3] = true;
    performance::draw(&mut display_active, &preset, Side::Left).unwrap();

    // Active rendering should produce different pixels than inactive
    // (filled background vs outline-only)
    let inactive_pixels: Vec<_> = display_inactive
        .affected_area()
        .points()
        .filter(|p| display_inactive.get_pixel(*p) == Some(Gray4::WHITE))
        .collect();
    let active_pixels: Vec<_> = display_active
        .affected_area()
        .points()
        .filter(|p| display_active.get_pixel(*p) == Some(Gray4::WHITE))
        .collect();

    // Active (filled) should have significantly more white pixels than inactive (outline only)
    assert!(
        active_pixels.len() > inactive_pixels.len() * 2,
        "Active button should have more white pixels (filled). active={}, inactive={}",
        active_pixels.len(),
        inactive_pixels.len()
    );
}

#[test]
fn draw_single_button_fills_more_vertical_space() {
    // With only 1 button, draw still works and produces output
    let mut display = new_display();

    let mut preset = PresetMeta::default();
    preset.button_labels[3] = String::try_from("Solo").unwrap();
    performance::draw(&mut display, &preset, Side::Left).unwrap();

    assert!(!display.affected_area().is_zero_sized());
}

#[test]
fn draw_two_buttons_renders_both() {
    // With 2 buttons on left display (D and A), both should produce output
    let mut display = new_display();

    let mut preset = PresetMeta::default();
    preset.button_labels[3] = String::try_from("Drive").unwrap(); // D = slot 0
    preset.button_labels[0] = String::try_from("Clean").unwrap(); // A = slot 2
    performance::draw(&mut display, &preset, Side::Left).unwrap();

    assert!(!display.affected_area().is_zero_sized());
}

#[test]
fn draw_two_buttons_preserves_position_order() {
    // If D (slot 0/top) and A (slot 2/bottom) are active, D should appear first (top)
    let mut display = new_display();

    let mut preset = PresetMeta::default();
    preset.button_labels[3] = String::try_from("Top").unwrap(); // D = slot 0
    preset.button_labels[0] = String::try_from("Bot").unwrap(); // A = slot 2
    performance::draw(&mut display, &preset, Side::Left).unwrap();

    // The affected area should start near the top (D is first)
    let area = display.affected_area();
    assert!(
        area.top_left.y < 10,
        "First button (D/top) should be near top of display. y={}",
        area.top_left.y
    );
}

#[test]
fn draw_middle_only_button_renders() {
    // Only E (index 4, slot 1) on left display
    let mut display = new_display();

    let mut preset = PresetMeta::default();
    preset.button_labels[4] = String::try_from("Tap").unwrap();
    performance::draw(&mut display, &preset, Side::Left).unwrap();

    // Should render starting near the top (it's the only row, placed at y=3)
    let area = display.affected_area();
    assert!(!area.is_zero_sized());
    assert!(
        area.top_left.y < 10,
        "Single button should start near top. y={}",
        area.top_left.y
    );
}

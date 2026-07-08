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
    let cfg = midi_controller::config::Config::default();
    let (name, labels) = performance::preset_meta_from_config(&cfg, 0);
    assert_eq!(name.as_str(), "Preset 1");
    assert_eq!(labels[0].as_str(), "A");
    assert_eq!(labels[5].as_str(), "F");
}

#[test]
fn preset_meta_defaults_for_index_beyond_vec() {
    let cfg = midi_controller::config::Config::default();
    let (name, _) = performance::preset_meta_from_config(&cfg, 4);
    assert_eq!(name.as_str(), "Preset 5");
}

#[test]
fn preset_meta_uses_config_name_and_labels() {
    use heapless::{String, Vec};
    use midi_controller::config::*;

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

    let cfg = Config { global: midi_controller::config::GlobalConfig::default(), presets };
    let (name, labels) = performance::preset_meta_from_config(&cfg, 0);
    assert_eq!(name.as_str(), "My Song");
    assert_eq!(labels[0].as_str(), "Verse");
    // Remaining buttons fall back to defaults
    assert_eq!(labels[1].as_str(), "B");
}

#[test]
fn preset_meta_empty_label_uses_default() {
    use heapless::{String, Vec};
    use midi_controller::config::*;

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

    let cfg = Config { global: midi_controller::config::GlobalConfig::default(), presets };
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
fn draw_single_button_at_fixed_position() {
    // Only button D (index 3) — should render at the top slot position
    let mut display = new_display();

    let mut preset = PresetMeta::default();
    preset.button_labels[3] = String::try_from("Solo").unwrap();
    performance::draw(&mut display, &preset, Side::Left).unwrap();

    let area = display.affected_area();
    assert!(!area.is_zero_sized());
    // Top slot starts at y=3 (PADDING)
    assert!(
        area.top_left.y < 5,
        "Top button should be at top position. y={}",
        area.top_left.y
    );
}

#[test]
fn draw_bottom_only_button_at_fixed_position() {
    // Only button A (index 0) on left display — should NOT render at the top
    // (bottom slot y=85 is beyond 64px MockDisplay, so no pixels within bounds)
    let mut display = new_display();

    let mut preset = PresetMeta::default();
    preset.button_labels[0] = String::try_from("Bass").unwrap();
    performance::draw(&mut display, &preset, Side::Left).unwrap();

    // Bottom slot starts at y=85 which is outside 64x64 MockDisplay
    // So affected_area should be empty (all drawing is out of bounds)
    assert!(
        display.affected_area().is_zero_sized(),
        "Bottom button (y=85) should be outside 64px MockDisplay bounds"
    );
}

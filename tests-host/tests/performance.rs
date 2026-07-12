// Host-side tests for src/views/performance.rs

#[path = "../../src/views/performance.rs"]
mod performance;

use embedded_graphics::{
    geometry::Size,
    pixelcolor::Gray4,
    pixelcolor::GrayColor,
    prelude::*,
};
use heapless::String;
use performance::{PresetMeta, Side};

/// 128×128 4-bit grayscale framebuffer — same geometry as the real SSD1327.
struct TestDisplay {
    framebuffer: [u8; 128 * 64], // 2 pixels per byte (4-bit each)
}

impl TestDisplay {
    fn new() -> Self {
        TestDisplay {
            framebuffer: [0u8; 128 * 64],
        }
    }

    /// Read a single pixel value at (x, y).
    fn get_pixel(&self, x: i32, y: i32) -> Gray4 {
        assert!(
            x >= 0 && x < 128 && y >= 0 && y < 128,
            "pixel out of bounds: ({}, {})",
            x,
            y
        );
        let index = (x / 2 + y * 64) as usize;
        let byte = self.framebuffer[index];
        let luma = if x % 2 == 0 {
            (byte >> 4) & 0x0F
        } else {
            byte & 0x0F
        };
        Gray4::new(luma)
    }

    /// Count non-black pixels in a row range (inclusive).
    fn count_visible_in_rows(&self, start_row: u8, end_row: u8) -> usize {
        let mut count = 0;
        for y in start_row as i32..=end_row as i32 {
            for x in 0..128i32 {
                if self.get_pixel(x, y) != Gray4::BLACK {
                    count += 1;
                }
            }
        }
        count
    }

    /// Count pixels that differ between this display and another in a row range.
    fn diff_in_rows(&self, other: &TestDisplay, start_row: u8, end_row: u8) -> usize {
        let mut count = 0;
        for y in start_row as i32..=end_row as i32 {
            for x in 0..128i32 {
                if self.get_pixel(x, y) != other.get_pixel(x, y) {
                    count += 1;
                }
            }
        }
        count
    }

    /// Count white pixels in a row range.
    fn count_white_in_rows(&self, start_row: u8, end_row: u8) -> usize {
        let mut count = 0;
        for y in start_row as i32..=end_row as i32 {
            for x in 0..128i32 {
                if self.get_pixel(x, y) == Gray4::WHITE {
                    count += 1;
                }
            }
        }
        count
    }

    /// Check if any pixel is non-black in the entire framebuffer.
    fn has_visible_pixels(&self) -> bool {
        self.framebuffer.iter().any(|&b| b != 0)
    }
}

impl DrawTarget for TestDisplay {
    type Color = Gray4;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels.into_iter() {
            if let Ok((x @ 0..=127, y @ 0..=127)) = coord.try_into() {
                let index: u32 = x / 2 + y * 64;
                let mut new_byte = color.luma();
                if x % 2 == 0 {
                    new_byte <<= 4;
                    self.framebuffer[index as usize] &= 0x0F;
                } else {
                    self.framebuffer[index as usize] &= 0xF0;
                }
                self.framebuffer[index as usize] |= new_byte;
            }
        }
        Ok(())
    }
}

impl OriginDimensions for TestDisplay {
    fn size(&self) -> Size {
        Size::new(128, 128)
    }
}

fn test_preset() -> PresetMeta {
    let mut p = PresetMeta::default();
    p.name = String::try_from("Clean+Delay").unwrap();
    p.preset_number = 1;
    p.button_labels[0] = String::try_from("Drive").unwrap();
    p.button_labels[1] = String::try_from("Delay").unwrap();
    p.button_labels[2] = String::try_from("Reverb").unwrap();
    p.button_labels[3] = String::try_from("Looper").unwrap();
    p.button_labels[4] = String::try_from("Tap").unwrap();
    p.button_labels[5] = String::try_from("Bank+").unwrap();
    p
}

// --- Basic rendering tests ---

#[test]
fn draw_left_renders_pixels() {
    let mut display = TestDisplay::new();
    let preset = test_preset();
    performance::draw(&mut display, &preset, Side::Left).unwrap();
    assert!(display.has_visible_pixels());
}

#[test]
fn draw_right_renders_pixels() {
    let mut display = TestDisplay::new();
    let preset = test_preset();
    performance::draw(&mut display, &preset, Side::Right).unwrap();
    assert!(display.has_visible_pixels());
}

#[test]
fn empty_labels_draws_nothing_on_right() {
    // Right display has no preset number, so empty labels = nothing drawn
    let mut display = TestDisplay::new();
    let preset = PresetMeta::default();
    performance::draw(&mut display, &preset, Side::Right).unwrap();
    assert!(!display.has_visible_pixels());
}

#[test]
fn empty_labels_only_draws_preset_number_on_left() {
    // Left display always draws preset number even with empty labels
    let mut display = TestDisplay::new();
    let mut preset = PresetMeta::default();
    preset.preset_number = 1;
    performance::draw(&mut display, &preset, Side::Left).unwrap();

    // Should have pixels only in the preset number region
    let (pn_start, pn_end) = performance::preset_number_flush_range();
    let total_visible = display.count_visible_in_rows(0, 127);
    let pn_visible = display.count_visible_in_rows(pn_start, pn_end);
    assert_eq!(total_visible, pn_visible, "Only preset number should be visible");
    assert!(pn_visible > 0, "Preset number should have pixels");
}

#[test]
fn draw_active_button_renders_filled() {
    let mut display_inactive = TestDisplay::new();
    let mut display_active = TestDisplay::new();

    let mut preset = test_preset();
    performance::draw(&mut display_inactive, &preset, Side::Left).unwrap();

    preset.button_active[3] = true;
    performance::draw(&mut display_active, &preset, Side::Left).unwrap();

    // Active (filled) should have significantly more white pixels than inactive (outline only)
    let (start, end) = performance::row_flush_range(0);
    let inactive_white = display_inactive.count_white_in_rows(start, end);
    let active_white = display_active.count_white_in_rows(start, end);
    assert!(
        active_white > inactive_white * 2,
        "Active button should have more white pixels (filled). active={}, inactive={}",
        active_white,
        inactive_white
    );
}

#[test]
fn draw_single_button_at_fixed_position() {
    let mut display = TestDisplay::new();
    let mut preset = PresetMeta::default();
    preset.button_labels[3] = String::try_from("Solo").unwrap();
    performance::draw(&mut display, &preset, Side::Left).unwrap();

    // Button D = left row 0, starts at y=PADDING=3
    let (start, end) = performance::row_flush_range(0);
    let row0_pixels = display.count_visible_in_rows(start, end);
    assert!(row0_pixels > 0, "Top button should have visible pixels in row 0");
}

#[test]
fn draw_bottom_button_at_correct_position() {
    let mut display = TestDisplay::new();
    let mut preset = PresetMeta::default();
    preset.button_labels[0] = String::try_from("Bass").unwrap();
    performance::draw(&mut display, &preset, Side::Left).unwrap();

    // Button A = left row 2
    let (start, end) = performance::row_flush_range(2);
    let row2_pixels = display.count_visible_in_rows(start, end);
    assert!(row2_pixels > 0, "Bottom button should have visible pixels in row 2");

    // Row 0 should be empty (no button D label)
    let (s0, e0) = performance::row_flush_range(0);
    let row0_pixels = display.count_visible_in_rows(s0, e0);
    assert_eq!(row0_pixels, 0, "Row 0 should be empty when only button A has a label");
}

#[test]
fn preset_number_renders_on_left_display() {
    let mut display = TestDisplay::new();
    let mut preset = test_preset();
    preset.preset_number = 3;
    performance::draw(&mut display, &preset, Side::Left).unwrap();

    let (start, end) = performance::preset_number_flush_range();
    let indicator_pixels = display.count_visible_in_rows(start, end);
    assert!(indicator_pixels > 0, "Preset number should render pixels");
}

#[test]
fn preset_number_not_rendered_on_right_display() {
    let mut display_left = TestDisplay::new();
    let mut display_right = TestDisplay::new();
    let mut preset = PresetMeta::default();
    preset.preset_number = 5;
    // No button labels — so the only thing drawn is the preset number
    performance::draw(&mut display_left, &preset, Side::Left).unwrap();
    performance::draw(&mut display_right, &preset, Side::Right).unwrap();

    let (start, end) = performance::preset_number_flush_range();
    let left_pixels = display_left.count_visible_in_rows(start, end);
    let right_pixels = display_right.count_visible_in_rows(start, end);
    assert!(left_pixels > 0, "Left display should show preset number");
    assert_eq!(right_pixels, 0, "Right display should NOT show preset number");
}

#[test]
fn long_press_hint_renders_indicator() {
    let mut display_with = TestDisplay::new();
    let mut display_without = TestDisplay::new();

    let mut preset = PresetMeta::default();
    preset.button_labels[5] = String::try_from("Next").unwrap();
    performance::draw(&mut display_without, &preset, Side::Right).unwrap();

    preset.long_press_hints[5] = String::try_from("» Next").unwrap();
    performance::draw(&mut display_with, &preset, Side::Right).unwrap();

    let (start, end) = performance::row_flush_range(0); // F = right row 0
    let without_white = display_without.count_white_in_rows(start, end);
    let with_white = display_with.count_white_in_rows(start, end);
    assert!(
        with_white > without_white,
        "Hint should add white pixels. with={}, without={}",
        with_white,
        without_white
    );
}

#[test]
fn long_press_hint_visible_on_active_button() {
    let mut display_active_no_hint = TestDisplay::new();
    let mut display_active_with_hint = TestDisplay::new();

    let mut preset = test_preset();
    preset.button_active[5] = true;
    performance::draw(&mut display_active_no_hint, &preset, Side::Right).unwrap();

    preset.long_press_hints[5] = String::try_from("» Next").unwrap();
    performance::draw(&mut display_active_with_hint, &preset, Side::Right).unwrap();

    // The hint indicator should create visual difference on the active button
    let (start, end) = performance::row_flush_range(0);
    let diff = display_active_no_hint.diff_in_rows(&display_active_with_hint, start, end);
    assert!(
        diff > 0,
        "Active button with hint should differ from active without hint"
    );
}

// --- Config metadata tests ---

#[test]
fn preset_meta_defaults_when_config_empty() {
    let cfg = midi_controller::config::Config::default();
    let (name, labels, _hints) = performance::preset_meta_from_config(&cfg, 0);
    assert_eq!(name.as_str(), "Preset 1");
    assert_eq!(labels[0].as_str(), "A");
    assert_eq!(labels[5].as_str(), "F");
}

#[test]
fn preset_meta_defaults_for_index_beyond_vec() {
    let cfg = midi_controller::config::Config::default();
    let (name, _, _) = performance::preset_meta_from_config(&cfg, 4);
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
            bpm: 0,
        })
        .ok();

    let cfg = Config {
        global: midi_controller::config::GlobalConfig::default(),
        presets,
    };
    let (name, labels, _hints) = performance::preset_meta_from_config(&cfg, 0);
    assert_eq!(name.as_str(), "My Song");
    assert_eq!(labels[0].as_str(), "Verse");
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
            label: String::new(),
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
            bpm: 0,
        })
        .ok();

    let cfg = Config {
        global: midi_controller::config::GlobalConfig::default(),
        presets,
    };
    let (_, labels, _) = performance::preset_meta_from_config(&cfg, 0);
    assert_eq!(labels[0].as_str(), "");
}

#[test]
fn preset_meta_from_config_includes_long_press_hints() {
    use heapless::{String, Vec};
    use midi_controller::config::*;

    let mut presets = Vec::new();
    let mut buttons = Vec::new();
    buttons
        .push(ButtonConfig {
            label: String::try_from("Next").unwrap(),
            color: LedConfig::default(),
            mode: ButtonMode::default(),
            on_press: Vec::new(),
            on_release: Vec::new(),
            on_long_press: {
                let mut v = Vec::new();
                v.push(Action::PresetNext).ok();
                v
            },
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
            bpm: 0,
        })
        .ok();

    let cfg = Config {
        global: GlobalConfig::default(),
        presets,
    };
    let (_, _, hints) = performance::preset_meta_from_config(&cfg, 0);
    assert!(
        hints[0].as_str().contains("Next"),
        "Expected hint to contain 'Next', got '{}'",
        hints[0]
    );
}

#[test]
fn preset_meta_from_config_prev_preset_hint() {
    use heapless::{String, Vec};
    use midi_controller::config::*;

    let mut presets = Vec::new();
    let mut buttons = Vec::new();
    buttons
        .push(ButtonConfig {
            label: String::try_from("Back").unwrap(),
            color: LedConfig::default(),
            mode: ButtonMode::default(),
            on_press: Vec::new(),
            on_release: Vec::new(),
            on_long_press: {
                let mut v = Vec::new();
                v.push(Action::PresetPrev).ok();
                v
            },
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
            bpm: 0,
        })
        .ok();

    let cfg = Config {
        global: GlobalConfig::default(),
        presets,
    };
    let (_, _, hints) = performance::preset_meta_from_config(&cfg, 0);
    assert!(
        hints[0].as_str().contains("Prev"),
        "Expected hint to contain 'Prev', got '{}'",
        hints[0]
    );
}

// --- Partial update (draw_row) tests ---

#[test]
fn draw_row_produces_same_pixels_as_full_draw_for_top_row() {
    let mut display_full = TestDisplay::new();
    let mut display_row = TestDisplay::new();

    let preset = test_preset();

    performance::draw(&mut display_full, &preset, Side::Left).unwrap();
    performance::draw_row(&mut display_row, &preset, Side::Left, 0).unwrap();

    let (start, end) = performance::row_flush_range(0);
    let mismatches = display_full.diff_in_rows(&display_row, start, end);
    assert_eq!(
        mismatches, 0,
        "draw_row(0) should match full draw in row 0 region"
    );
}

#[test]
fn draw_row_produces_same_pixels_as_full_draw_for_mid_row() {
    let mut display_full = TestDisplay::new();
    let mut display_row = TestDisplay::new();

    let preset = test_preset();

    performance::draw(&mut display_full, &preset, Side::Left).unwrap();
    performance::draw_row(&mut display_row, &preset, Side::Left, 1).unwrap();

    let (start, end) = performance::row_flush_range(1);
    let mismatches = display_full.diff_in_rows(&display_row, start, end);
    assert_eq!(
        mismatches, 0,
        "draw_row(1) should match full draw in row 1 region"
    );
}

#[test]
fn draw_row_produces_same_pixels_as_full_draw_for_bottom_row() {
    let mut display_full = TestDisplay::new();
    let mut display_row = TestDisplay::new();

    let preset = test_preset();

    performance::draw(&mut display_full, &preset, Side::Left).unwrap();
    performance::draw_row(&mut display_row, &preset, Side::Left, 2).unwrap();

    // Row 2 flush range overlaps with the preset number indicator on the left display.
    // draw_row does NOT render the preset number (that's a separate call).
    // Compare only the portion above the preset number area.
    let (start, _end) = performance::row_flush_range(2);
    let (pn_start, _) = performance::preset_number_flush_range();
    // Compare only the non-overlapping part of the row
    let safe_end = pn_start - 1;
    let mismatches = display_full.diff_in_rows(&display_row, start, safe_end);
    assert_eq!(
        mismatches, 0,
        "draw_row(2) should match full draw in row 2 region (above preset number)"
    );
}

#[test]
fn draw_row_bottom_right_matches_full_draw() {
    // Right display has no preset number, so bottom row should fully match
    let mut display_full = TestDisplay::new();
    let mut display_row = TestDisplay::new();

    let preset = test_preset();

    performance::draw(&mut display_full, &preset, Side::Right).unwrap();
    performance::draw_row(&mut display_row, &preset, Side::Right, 2).unwrap();

    let (start, end) = performance::row_flush_range(2);
    let mismatches = display_full.diff_in_rows(&display_row, start, end);
    assert_eq!(
        mismatches, 0,
        "draw_row(2) on right display should fully match (no preset number overlap)"
    );
}

#[test]
fn draw_row_produces_same_pixels_as_full_draw_right_display() {
    let mut display_full = TestDisplay::new();
    let mut display_row = TestDisplay::new();

    let preset = test_preset();

    performance::draw(&mut display_full, &preset, Side::Right).unwrap();
    performance::draw_row(&mut display_row, &preset, Side::Right, 0).unwrap();

    let (start, end) = performance::row_flush_range(0);
    let mismatches = display_full.diff_in_rows(&display_row, start, end);
    assert_eq!(
        mismatches, 0,
        "draw_row(0) on right display should match full draw"
    );
}

#[test]
fn draw_row_does_not_affect_other_rows() {
    let mut display = TestDisplay::new();
    let preset = test_preset();

    // Draw only row 0
    performance::draw_row(&mut display, &preset, Side::Left, 0).unwrap();

    // Row 1 should be untouched
    let (s1, e1) = performance::row_flush_range(1);
    let row1_pixels = display.count_visible_in_rows(s1, e1);
    assert_eq!(row1_pixels, 0, "draw_row(0) should not touch row 1 pixels");

    // Row 2 should be untouched
    let (s2, e2) = performance::row_flush_range(2);
    let row2_pixels = display.count_visible_in_rows(s2, e2);
    assert_eq!(row2_pixels, 0, "draw_row(0) should not touch row 2 pixels");
}

#[test]
fn draw_row_mid_does_not_affect_adjacent_rows() {
    let mut display = TestDisplay::new();
    let preset = test_preset();

    performance::draw_row(&mut display, &preset, Side::Left, 1).unwrap();

    let (s0, e0) = performance::row_flush_range(0);
    let row0_pixels = display.count_visible_in_rows(s0, e0);
    assert_eq!(row0_pixels, 0, "draw_row(1) should not touch row 0 pixels");

    let (s2, e2) = performance::row_flush_range(2);
    let row2_pixels = display.count_visible_in_rows(s2, e2);
    assert_eq!(row2_pixels, 0, "draw_row(1) should not touch row 2 pixels");
}

#[test]
fn draw_row_active_state_differs_from_inactive() {
    let mut display_inactive = TestDisplay::new();
    let mut display_active = TestDisplay::new();

    let mut preset = test_preset();
    performance::draw_row(&mut display_inactive, &preset, Side::Left, 0).unwrap();

    preset.button_active[3] = true; // D = left row 0
    performance::draw_row(&mut display_active, &preset, Side::Left, 0).unwrap();

    let (start, end) = performance::row_flush_range(0);
    let differing = display_inactive.diff_in_rows(&display_active, start, end);
    assert!(
        differing > 100,
        "Active and inactive row should differ significantly. differing={}",
        differing
    );
}

#[test]
fn draw_row_clears_previous_content() {
    let mut display_redraw = TestDisplay::new();
    let mut display_fresh = TestDisplay::new();

    let mut preset = test_preset();

    // Draw as active first
    preset.button_active[3] = true;
    performance::draw_row(&mut display_redraw, &preset, Side::Left, 0).unwrap();

    // Redraw as inactive (draw_row should clear the region first)
    preset.button_active[3] = false;
    performance::draw_row(&mut display_redraw, &preset, Side::Left, 0).unwrap();

    // Fresh inactive draw for comparison
    performance::draw_row(&mut display_fresh, &preset, Side::Left, 0).unwrap();

    let (start, end) = performance::row_flush_range(0);
    let mismatches = display_redraw.diff_in_rows(&display_fresh, start, end);
    assert_eq!(
        mismatches, 0,
        "Redrawing inactive over active should match fresh inactive draw"
    );
}

#[test]
fn draw_row_with_long_press_hint() {
    let mut display_with = TestDisplay::new();
    let mut display_without = TestDisplay::new();

    let mut preset = test_preset();
    performance::draw_row(&mut display_without, &preset, Side::Right, 0).unwrap();

    preset.long_press_hints[5] = String::try_from("» Next").unwrap(); // F = right row 0
    performance::draw_row(&mut display_with, &preset, Side::Right, 0).unwrap();

    let (start, end) = performance::row_flush_range(0);
    let differing = display_with.diff_in_rows(&display_without, start, end);
    assert!(
        differing > 10,
        "Long-press hint should add visible pixels. differing={}",
        differing
    );
}

#[test]
fn draw_row_empty_label_draws_nothing_in_row() {
    let mut display = TestDisplay::new();
    let mut preset = PresetMeta::default();
    preset.button_labels[3] = String::try_from("Solo").unwrap();

    // Draw row 1 (button E, index 4) which has empty label
    performance::draw_row(&mut display, &preset, Side::Left, 1).unwrap();

    let (start, end) = performance::row_flush_range(1);
    let row_pixels = display.count_visible_in_rows(start, end);
    assert_eq!(row_pixels, 0, "Empty label row should have no visible pixels");
}

// --- Geometry / helper tests ---

#[test]
fn row_flush_range_covers_correct_regions() {
    let (s0, e0) = performance::row_flush_range(0);
    let (s1, e1) = performance::row_flush_range(1);
    let (s2, e2) = performance::row_flush_range(2);

    // No overlap between rows
    assert!(e0 < s1, "Row 0 end ({}) should be before row 1 start ({})", e0, s1);
    assert!(e1 < s2, "Row 1 end ({}) should be before row 2 start ({})", e1, s2);

    // Each row should cover at least ROW_HEIGHT pixels
    assert!(e0 - s0 >= 38, "Row 0 range too small: {}", e0 - s0);
    assert!(e1 - s1 >= 38, "Row 1 range too small: {}", e1 - s1);
    assert!(e2 - s2 >= 38, "Row 2 range too small: {}", e2 - s2);

    // Should stay within display bounds
    assert!(e2 <= 127, "Row 2 end should be within display: {}", e2);
}

#[test]
fn button_to_row_mapping_is_correct() {
    assert!(matches!(performance::button_to_row(0), (Side::Left, 2)));  // A
    assert!(matches!(performance::button_to_row(1), (Side::Right, 1))); // B
    assert!(matches!(performance::button_to_row(2), (Side::Right, 2))); // C
    assert!(matches!(performance::button_to_row(3), (Side::Left, 0)));  // D
    assert!(matches!(performance::button_to_row(4), (Side::Left, 1)));  // E
    assert!(matches!(performance::button_to_row(5), (Side::Right, 0))); // F
}

#[test]
fn preset_number_flush_range_is_within_display() {
    let (start, end) = performance::preset_number_flush_range();
    assert!(start < end);
    assert!(end <= 127);
    assert!(start >= 100, "Preset number should be near bottom: start={}", start);
}

// --- Full draw equivalence (all rows combined) ---

#[test]
fn all_rows_combined_match_full_draw_left() {
    // Drawing all 3 rows individually should produce the same result as full draw
    // (except for the preset number indicator, which is separate).
    let mut display_full = TestDisplay::new();
    let mut display_rows = TestDisplay::new();

    let preset = test_preset();

    performance::draw(&mut display_full, &preset, Side::Left).unwrap();

    performance::draw_row(&mut display_rows, &preset, Side::Left, 0).unwrap();
    performance::draw_row(&mut display_rows, &preset, Side::Left, 1).unwrap();
    performance::draw_row(&mut display_rows, &preset, Side::Left, 2).unwrap();
    performance::draw_preset_number(&mut display_rows, preset.preset_number).unwrap();

    // Compare entire framebuffer
    let mismatches = display_full.diff_in_rows(&display_rows, 0, 127);
    assert_eq!(
        mismatches, 0,
        "All rows + preset number should equal full draw"
    );
}

#[test]
fn draw_row_2_then_preset_number_preserves_indicator() {
    // Regression: drawing row 2 (button A) clears the preset number area.
    // The correct sequence is: draw_row(2) then draw_preset_number().
    let mut display = TestDisplay::new();
    let preset = test_preset();

    // Draw row 2 which overlaps with preset number region
    performance::draw_row(&mut display, &preset, Side::Left, 2).unwrap();
    // Re-draw preset number on top
    performance::draw_preset_number(&mut display, preset.preset_number).unwrap();

    // Preset number should be visible
    let (pn_start, pn_end) = performance::preset_number_flush_range();
    let pn_pixels = display.count_visible_in_rows(pn_start, pn_end);
    assert!(
        pn_pixels > 0,
        "Preset number must be visible after row 2 redraw + preset_number redraw"
    );
}

#[test]
fn draw_row_2_without_preset_number_erases_it() {
    // Confirms that draw_row(2) alone DOES erase the preset number
    // (this is the bug scenario if preset_number isn't redrawn after).
    let mut display = TestDisplay::new();
    let mut preset = test_preset();
    preset.preset_number = 5;

    // First draw full (includes preset number)
    performance::draw(&mut display, &preset, Side::Left).unwrap();
    let (pn_start, pn_end) = performance::preset_number_flush_range();
    let before = display.count_visible_in_rows(pn_start, pn_end);
    assert!(before > 0, "Preset number should be visible after full draw");

    // Now redraw only row 2 without re-drawing preset number
    performance::draw_row(&mut display, &preset, Side::Left, 2).unwrap();
    let after = display.count_visible_in_rows(pn_start, pn_end);

    // Some preset number pixels should be erased (the overlap portion)
    assert!(
        after < before,
        "draw_row(2) without preset_number should erase some indicator pixels. before={}, after={}",
        before,
        after
    );
}

#[test]
fn all_rows_combined_match_full_draw_right() {
    let mut display_full = TestDisplay::new();
    let mut display_rows = TestDisplay::new();

    let preset = test_preset();

    performance::draw(&mut display_full, &preset, Side::Right).unwrap();

    performance::draw_row(&mut display_rows, &preset, Side::Right, 0).unwrap();
    performance::draw_row(&mut display_rows, &preset, Side::Right, 1).unwrap();
    performance::draw_row(&mut display_rows, &preset, Side::Right, 2).unwrap();

    let mismatches = display_full.diff_in_rows(&display_rows, 0, 127);
    assert_eq!(
        mismatches, 0,
        "All rows combined should equal full draw on right display"
    );
}

#[test]
fn all_rows_combined_with_active_buttons_match_full_draw() {
    let mut display_full = TestDisplay::new();
    let mut display_rows = TestDisplay::new();

    let mut preset = test_preset();
    preset.button_active[3] = true; // D active
    preset.button_active[1] = true; // B active
    preset.long_press_hints[0] = String::try_from("« Prev").unwrap();
    preset.long_press_hints[5] = String::try_from("» Next").unwrap();

    performance::draw(&mut display_full, &preset, Side::Left).unwrap();

    performance::draw_row(&mut display_rows, &preset, Side::Left, 0).unwrap();
    performance::draw_row(&mut display_rows, &preset, Side::Left, 1).unwrap();
    performance::draw_row(&mut display_rows, &preset, Side::Left, 2).unwrap();
    performance::draw_preset_number(&mut display_rows, preset.preset_number).unwrap();

    let mismatches = display_full.diff_in_rows(&display_rows, 0, 127);
    assert_eq!(
        mismatches, 0,
        "Partial draw with active buttons + hints should equal full draw"
    );
}

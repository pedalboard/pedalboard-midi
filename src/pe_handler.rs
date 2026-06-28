//! PE preset event handler: thin hardware adapter over protocol::engine.
//!
//! Responsibilities (HMI/hardware only):
//! - Long-press detection (tick-based timing)
//! - Encoder acceleration (tick intervals → step count)
//! - Input event → abstract index mapping
//! - Format conversion (MidiMessage → raw bytes)

use crate::events::{Edge, InputEvent, Pulse};
use crate::ledring::Animation;
use crate::long_press::{Gesture, LongPressDetector};
use pedalboard_protocol::action::{EncoderDirection, MidiMessage};
use pedalboard_protocol::config::{ButtonMode, Color, Preset};
use pedalboard_protocol::engine::{self, ButtonEvent};
use pedalboard_protocol::state::{PresetState, PresetStateStore};
use smart_leds::RGB8;

const NUM_BUTTONS: usize = 6;
/// ADC upper trim — hardware doesn't reach full 4095.
const ADC_MAX_TRIMMED: u16 = 3750;

// Re-export types used by main.rs
pub use pedalboard_protocol::engine::{DisplayEvent, DisplaySide, SystemAction};

/// Result of processing events: MIDI messages + system actions + display + LED dirty flag.
pub struct HandleResult {
    pub midi: heapless::Vec<([u8; 3], usize), 8>,
    pub system: heapless::Vec<SystemAction, 2>,
    pub display: heapless::Vec<DisplayEvent, 2>,
    pub led_dirty: bool,
}

/// LED state for all 8 rings (A-F + Vol + Gain).
pub type LedAnimations = [Animation; 8];

/// Stateful PE event handler. Thin adapter over protocol::engine.
pub struct PeHandler {
    pub encoder_values: [u8; 2],
    button_active: [bool; NUM_BUTTONS],
    cycle_index: [u8; NUM_BUTTONS],
    last_encoder_tick: [u16; 2],
    long_press: [LongPressDetector; NUM_BUTTONS],
    state_store: PresetStateStore,
}

impl Default for PeHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl PeHandler {
    pub fn new() -> Self {
        Self {
            encoder_values: [0; 2],
            button_active: [false; NUM_BUTTONS],
            cycle_index: [0; NUM_BUTTONS],
            last_encoder_tick: [u16::MAX; 2],
            long_press: core::array::from_fn(|_| LongPressDetector::new()),
            state_store: PresetStateStore::new(),
        }
    }

    /// Switch to a new preset: saves current state, loads new state,
    /// and returns MIDI messages to sync external gear.
    pub fn switch_preset(
        &mut self,
        new_preset: u8,
        old_preset: &Preset,
        new_preset_cfg: &Preset,
    ) -> heapless::Vec<([u8; 3], usize), 16> {
        // Clear momentary visual feedback — only toggle/radio state should persist
        for i in 0..NUM_BUTTONS {
            if let Some(btn) = old_preset.buttons.get(i) {
                if matches!(btn.mode, ButtonMode::Momentary) {
                    self.button_active[i] = false;
                }
            }
        }

        let mut working = self.working_state();
        let recall = self
            .state_store
            .switch(new_preset, &mut working, new_preset_cfg);
        self.apply_state(&working);
        self.long_press = core::array::from_fn(|_| LongPressDetector::new());

        let mut result = heapless::Vec::new();
        for msg in &recall {
            result.push(midi_to_raw(msg)).ok();
        }
        result
    }

    /// Call every 1ms unconditionally to keep encoder acceleration timing accurate.
    pub fn tick(&mut self) {
        self.last_encoder_tick[0] = self.last_encoder_tick[0].saturating_add(1);
        self.last_encoder_tick[1] = self.last_encoder_tick[1].saturating_add(1);
    }

    /// Returns true if any button is currently held (long-press counting).
    pub fn any_active(&self) -> bool {
        self.long_press.iter().any(|lp| lp.is_active())
    }

    /// Process input events against a PE preset. Returns MIDI messages and system actions.
    pub fn handle_events(&mut self, preset: &Preset, events: &[InputEvent]) -> HandleResult {
        let mut midi = heapless::Vec::new();
        let mut system = heapless::Vec::new();
        let mut display = heapless::Vec::new();
        let mut led_dirty = false;

        // --- Buttons: long-press detection (HMI) → engine (logic) ---
        for i in 0..NUM_BUTTONS {
            let edge = button_edge(events, i);

            let has_long_press = preset
                .buttons
                .get(i)
                .map(|b| !b.on_long_press.is_empty())
                .unwrap_or(false);

            let mode = preset
                .buttons
                .get(i)
                .map(|b| &b.mode)
                .unwrap_or(&ButtonMode::Momentary);

            if has_long_press {
                // Momentary visual feedback while held
                if matches!(mode, &ButtonMode::Momentary) {
                    match edge {
                        Some(Edge::Activate) => {
                            self.button_active[i] = true;
                            led_dirty = true;
                        }
                        Some(Edge::Deactivate) => {
                            self.button_active[i] = false;
                            led_dirty = true;
                        }
                        None => {}
                    }
                }
                // Long-press detection resolves gesture
                match self.long_press[i].update(edge) {
                    Some(Gesture::ShortPress) => {
                        let mut state = self.working_state();
                        let r = engine::process_button(&mut state, preset, i, ButtonEvent::Press);
                        self.apply_state(&state);
                        self.merge_result(&r, &mut midi, &mut system, &mut display, &mut led_dirty);
                        // For momentary: also release (button is no longer held)
                        // For toggle/radio with on_release: fire release actions
                        if let Some(btn) = preset.buttons.get(i) {
                            if mode == &ButtonMode::Momentary || !btn.on_release.is_empty() {
                                let mut state2 = self.working_state();
                                let r2 = engine::process_button(
                                    &mut state2,
                                    preset,
                                    i,
                                    ButtonEvent::Release,
                                );
                                self.apply_state(&state2);
                                self.merge_result(
                                    &r2,
                                    &mut midi,
                                    &mut system,
                                    &mut display,
                                    &mut led_dirty,
                                );
                            }
                        }
                    }
                    Some(Gesture::LongPress) => {
                        let mut state = self.working_state();
                        let r =
                            engine::process_button(&mut state, preset, i, ButtonEvent::LongPress);
                        self.apply_state(&state);
                        self.merge_result(&r, &mut midi, &mut system, &mut display, &mut led_dirty);
                    }
                    None => {}
                }
            } else {
                // No long-press: immediate dispatch
                match edge {
                    Some(Edge::Activate) => {
                        let mut state = self.working_state();
                        let r = engine::process_button(&mut state, preset, i, ButtonEvent::Press);
                        self.apply_state(&state);
                        self.merge_result(&r, &mut midi, &mut system, &mut display, &mut led_dirty);
                    }
                    Some(Edge::Deactivate) => {
                        let mut state = self.working_state();
                        let r = engine::process_button(&mut state, preset, i, ButtonEvent::Release);
                        self.apply_state(&state);
                        self.merge_result(&r, &mut midi, &mut system, &mut display, &mut led_dirty);
                    }
                    None => {}
                }
            }
        }

        // --- Encoders: acceleration (HMI) → engine (logic) ---
        for event in events {
            match event {
                InputEvent::Vol(pulse) => {
                    let steps = accel_steps(self.last_encoder_tick[0]);
                    self.last_encoder_tick[0] = 0;
                    let mut state = self.working_state();
                    let r =
                        engine::process_encoder(&mut state, preset, 0, pulse_to_dir(*pulse), steps);
                    self.apply_state(&state);
                    self.merge_result(&r, &mut midi, &mut system, &mut display, &mut led_dirty);
                }
                InputEvent::Gain(pulse) => {
                    let steps = accel_steps(self.last_encoder_tick[1]);
                    self.last_encoder_tick[1] = 0;
                    let mut state = self.working_state();
                    let r =
                        engine::process_encoder(&mut state, preset, 1, pulse_to_dir(*pulse), steps);
                    self.apply_state(&state);
                    self.merge_result(&r, &mut midi, &mut system, &mut display, &mut led_dirty);
                }
                InputEvent::ExpressionPedalA(raw_adc) => {
                    let adc = (*raw_adc).min(ADC_MAX_TRIMMED);
                    let r = engine::process_analog(preset, 0, adc, ADC_MAX_TRIMMED);
                    self.merge_result(&r, &mut midi, &mut system, &mut display, &mut led_dirty);
                }
                InputEvent::ExpressionPedalB(raw_adc) => {
                    let adc = (*raw_adc).min(ADC_MAX_TRIMMED);
                    let r = engine::process_analog(preset, 1, adc, ADC_MAX_TRIMMED);
                    self.merge_result(&r, &mut midi, &mut system, &mut display, &mut led_dirty);
                }
                _ => {}
            }
        }

        HandleResult {
            midi,
            system,
            display,
            led_dirty,
        }
    }

    /// Compute LED animations for all 8 rings based on current state + preset config.
    pub fn led_state(&self, preset: &Preset) -> LedAnimations {
        let mut anims = [Animation::Off; 8];

        for (i, anim) in anims.iter_mut().enumerate().take(NUM_BUTTONS) {
            if let Some(btn) = preset.buttons.get(i) {
                let color = if self.button_active[i] {
                    color_to_rgb(&btn.color.on)
                } else if btn.color.off == Color::Off {
                    let on = color_to_rgb(&btn.color.on);
                    RGB8::new(on.r / 6, on.g / 6, on.b / 6)
                } else {
                    color_to_rgb(&btn.color.off)
                };
                *anim = if color == RGB8::default() {
                    Animation::Off
                } else {
                    Animation::On(color)
                };
            }
        }

        let fill_vol = ((self.encoder_values[0] as u16 * 12) / 127).min(12) as u8;
        anims[6] = Animation::Heatmap(fill_vol);
        let fill_gain = ((self.encoder_values[1] as u16 * 12) / 127).min(12) as u8;
        anims[7] = Animation::Heatmap(fill_gain);

        anims
    }

    // --- Private helpers ---

    fn working_state(&self) -> PresetState {
        PresetState {
            button_active: self.button_active,
            cycle_index: self.cycle_index,
            encoder_values: self.encoder_values,
        }
    }

    fn apply_state(&mut self, state: &PresetState) {
        self.button_active = state.button_active;
        self.cycle_index = state.cycle_index;
        self.encoder_values = state.encoder_values;
    }

    fn merge_result(
        &self,
        r: &engine::EngineResult,
        midi: &mut heapless::Vec<([u8; 3], usize), 8>,
        system: &mut heapless::Vec<SystemAction, 2>,
        display: &mut heapless::Vec<DisplayEvent, 2>,
        led_dirty: &mut bool,
    ) {
        for msg in &r.midi {
            midi.push(midi_to_raw(msg)).ok();
        }
        for s in &r.system {
            system.push(*s).ok();
        }
        for d in &r.display {
            display.push(d.clone()).ok();
        }
        if r.led_dirty {
            *led_dirty = true;
        }
    }
}

fn button_edge(events: &[InputEvent], i: usize) -> Option<Edge> {
    events.iter().find_map(|e| match (e, i) {
        (InputEvent::ButtonA(e), 0) => Some(*e),
        (InputEvent::ButtonB(e), 1) => Some(*e),
        (InputEvent::ButtonC(e), 2) => Some(*e),
        (InputEvent::ButtonD(e), 3) => Some(*e),
        (InputEvent::ButtonE(e), 4) => Some(*e),
        (InputEvent::ButtonF(e), 5) => Some(*e),
        _ => None,
    })
}

fn color_to_rgb(c: &Color) -> RGB8 {
    match c {
        Color::Off => RGB8::new(0, 0, 0),
        Color::Red => RGB8::new(255, 0, 0),
        Color::Green => RGB8::new(0, 255, 0),
        Color::Blue => RGB8::new(0, 0, 255),
        Color::Yellow => RGB8::new(255, 255, 0),
        Color::Cyan => RGB8::new(0, 255, 255),
        Color::Magenta => RGB8::new(255, 0, 255),
        Color::White => RGB8::new(255, 255, 255),
        Color::Orange => RGB8::new(255, 128, 0),
        Color::Purple => RGB8::new(128, 0, 255),
        Color::Custom(r, g, b) => RGB8::new(*r, *g, *b),
    }
}

fn pulse_to_dir(pulse: Pulse) -> EncoderDirection {
    match pulse {
        Pulse::Clockwise => EncoderDirection::Clockwise,
        Pulse::CounterClockwise => EncoderDirection::CounterClockwise,
    }
}

/// Encoder acceleration: faster turning = bigger steps.
fn accel_steps(ticks_since_last: u16) -> u8 {
    if ticks_since_last < 20 {
        8
    } else if ticks_since_last < 50 {
        4
    } else if ticks_since_last < 100 {
        2
    } else {
        1
    }
}

fn midi_to_raw(msg: &MidiMessage) -> ([u8; 3], usize) {
    let mut raw = [0u8; 3];
    let len = msg.len.min(3);
    raw[..len].copy_from_slice(&msg.data[..len]);
    (raw, len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use heapless::Vec;
    use pedalboard_protocol::config::*;

    fn make_test_preset() -> Preset {
        let mut buttons: Vec<ButtonConfig, MAX_BUTTONS> = Vec::new();
        let mut on_press: Vec<Action, MAX_ACTIONS> = Vec::new();
        on_press
            .push(Action::NoteOn {
                note: 60,
                channel: 1,
            })
            .ok();
        let mut on_release: Vec<Action, MAX_ACTIONS> = Vec::new();
        on_release
            .push(Action::NoteOff {
                note: 60,
                channel: 1,
            })
            .ok();
        buttons
            .push(ButtonConfig {
                label: Label::new(),
                color: LedConfig::default(),
                mode: ButtonMode::default(),
                on_press,
                on_release,
                on_long_press: Vec::new(),
                cycle_values: Vec::new(),
            })
            .ok();

        let mut on_press_b: Vec<Action, MAX_ACTIONS> = Vec::new();
        on_press_b
            .push(Action::Cc {
                cc: 10,
                value: 127,
                channel: 1,
            })
            .ok();
        let mut on_long_press_b: Vec<Action, MAX_ACTIONS> = Vec::new();
        on_long_press_b
            .push(Action::ProgramChange {
                program: 5,
                channel: 1,
            })
            .ok();
        buttons
            .push(ButtonConfig {
                label: Label::new(),
                color: LedConfig::default(),
                mode: ButtonMode::default(),
                on_press: on_press_b,
                on_release: Vec::new(),
                on_long_press: on_long_press_b,
                cycle_values: Vec::new(),
            })
            .ok();

        let mut encoders: Vec<EncoderConfig, MAX_ENCODERS> = Vec::new();
        encoders
            .push(EncoderConfig {
                label: Label::try_from("Vol").unwrap(),
                action: EncoderAction::Cc {
                    cc: 7,
                    channel: 1,
                    min: 0,
                    max: 127,
                },
            })
            .ok();

        Preset {
            name: Label::try_from("Test").unwrap(),
            buttons,
            encoders,
            analog: Vec::new(),
        }
    }

    #[test]
    fn on_press_fires_immediately_without_long_press() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        let r = handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
        assert_eq!(r.midi.len(), 1);
        assert_eq!(r.midi[0].0, [0x90, 60, 127]);
    }

    #[test]
    fn on_release_fires_on_deactivate() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        let r = handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Deactivate)]);
        assert_eq!(r.midi.len(), 1);
        assert_eq!(r.midi[0].0, [0x80, 60, 0]);
    }

    #[test]
    fn long_press_button_defers_on_press() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        let r = handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        assert!(r.midi.is_empty());
    }

    #[test]
    fn long_press_short_release_fires_on_press() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        for _ in 0..100 {
            handler.handle_events(&preset, &[]);
        }
        let r = handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
        assert_eq!(r.midi.len(), 1);
        assert_eq!(r.midi[0].0, [0xB0, 10, 127]);
    }

    #[test]
    fn long_press_fires_on_long_press() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        let mut found = false;
        for _ in 0..501 {
            let r = handler.handle_events(&preset, &[]);
            if !r.midi.is_empty() {
                assert_eq!(r.midi[0].0[..2], [0xC0, 5]);
                found = true;
                break;
            }
        }
        assert!(found);
    }

    #[test]
    fn long_press_suppresses_on_press_on_release() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        for _ in 0..501 {
            handler.handle_events(&preset, &[]);
        }
        let r = handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
        assert!(r.midi.is_empty());
    }

    #[test]
    fn momentary_with_long_press_led_off_after_short_release() {
        // Button B: momentary + has on_long_press
        let preset = make_test_preset();
        let mut handler = PeHandler::new();

        // Press and release quickly (short press)
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        assert!(handler.button_active[1]); // LED on while held

        // Tick a few times (not enough for long press)
        for _ in 0..100 {
            handler.handle_events(&preset, &[]);
        }

        // Release — should fire on_press AND clear LED
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
        assert!(!handler.button_active[1]); // LED must be off after release
    }

    #[test]
    fn long_press_switch_clears_held_button_led() {
        // Button B has on_long_press (PresetNext). After long-press triggers
        // a preset switch, the new preset should NOT show button B as active.
        let preset = make_test_preset();
        let mut handler = PeHandler::new();

        // Hold button B
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        assert!(handler.button_active[1]); // visual feedback while held

        // Tick past threshold — long press fires
        for _ in 0..501 {
            handler.handle_events(&preset, &[]);
        }

        // Now simulate what poll_input does: switch_preset
        handler.switch_preset(1, &preset, &preset);

        // New preset should have clean state — button B not active
        assert!(!handler.button_active[1]);
    }

    #[test]
    fn long_press_switch_does_not_save_held_button_as_active() {
        // When switching away via long-press, the held button's visual feedback
        // should NOT be persisted. Switching back should show it inactive.
        let preset = make_test_preset();
        let mut handler = PeHandler::new();

        // Hold button B and trigger long-press to switch to preset 1
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        for _ in 0..501 {
            handler.handle_events(&preset, &[]);
        }
        handler.switch_preset(1, &preset, &preset);

        // Switch back to preset 0 — button B should NOT be active
        handler.switch_preset(0, &preset, &preset);
        assert!(!handler.button_active[1]);
    }

    #[test]
    fn toggle_state_preserved_across_switch_via_different_button() {
        // P1: button A=toggle, button B=momentary+long_press(switch)
        // 1. Short-press A (toggle ON)
        // 2. Long-press B to switch to preset 1
        // 3. Long-press B to switch back to preset 0
        // => A should still be toggle ON
        use pedalboard_protocol::config::*;

        let mut buttons_p1: heapless::Vec<ButtonConfig, MAX_BUTTONS> = heapless::Vec::new();
        // A: Toggle
        let mut on_press_a: heapless::Vec<Action, MAX_ACTIONS> = heapless::Vec::new();
        on_press_a
            .push(Action::Cc {
                cc: 0,
                value: 127,
                channel: 1,
            })
            .ok();
        buttons_p1
            .push(ButtonConfig {
                label: Label::new(),
                color: LedConfig::default(),
                mode: ButtonMode::Toggle,
                on_press: on_press_a,
                on_release: heapless::Vec::new(),
                on_long_press: heapless::Vec::new(),
                cycle_values: heapless::Vec::new(),
            })
            .ok();
        // B: Momentary + long_press = next_preset
        let mut on_long_b: heapless::Vec<Action, MAX_ACTIONS> = heapless::Vec::new();
        on_long_b.push(Action::PresetNext).ok();
        buttons_p1
            .push(ButtonConfig {
                label: Label::new(),
                color: LedConfig::default(),
                mode: ButtonMode::Momentary,
                on_press: heapless::Vec::new(),
                on_release: heapless::Vec::new(),
                on_long_press: on_long_b.clone(),
                cycle_values: heapless::Vec::new(),
            })
            .ok();

        let preset_p1 = Preset {
            name: Label::try_from("P1").unwrap(),
            buttons: buttons_p1,
            encoders: heapless::Vec::new(),
            analog: heapless::Vec::new(),
        };

        // P2: B also has long_press (to switch back)
        let mut buttons_p2: heapless::Vec<ButtonConfig, MAX_BUTTONS> = heapless::Vec::new();
        buttons_p2
            .push(ButtonConfig {
                label: Label::new(),
                color: LedConfig::default(),
                mode: ButtonMode::Momentary,
                on_press: heapless::Vec::new(),
                on_release: heapless::Vec::new(),
                on_long_press: heapless::Vec::new(),
                cycle_values: heapless::Vec::new(),
            })
            .ok();
        buttons_p2
            .push(ButtonConfig {
                label: Label::new(),
                color: LedConfig::default(),
                mode: ButtonMode::Momentary,
                on_press: heapless::Vec::new(),
                on_release: heapless::Vec::new(),
                on_long_press: on_long_b,
                cycle_values: heapless::Vec::new(),
            })
            .ok();

        let preset_p2 = Preset {
            name: Label::try_from("P2").unwrap(),
            buttons: buttons_p2,
            encoders: heapless::Vec::new(),
            analog: heapless::Vec::new(),
        };

        let mut handler = PeHandler::new();

        // 1. Short-press A on P1 (toggle ON)
        handler.handle_events(&preset_p1, &[InputEvent::ButtonA(Edge::Activate)]);
        handler.handle_events(&preset_p1, &[InputEvent::ButtonA(Edge::Deactivate)]);
        assert!(handler.button_active[0]); // toggled ON

        // 2. Long-press B to switch to P2
        handler.handle_events(&preset_p1, &[InputEvent::ButtonB(Edge::Activate)]);
        for _ in 0..501 {
            handler.handle_events(&preset_p1, &[]);
        }
        // Simulate poll_input: switch_preset
        handler.switch_preset(1, &preset_p1, &preset_p2);

        // 3. Long-press B to switch back to P1
        handler.handle_events(&preset_p2, &[InputEvent::ButtonB(Edge::Activate)]);
        for _ in 0..501 {
            handler.handle_events(&preset_p2, &[]);
        }
        handler.switch_preset(0, &preset_p2, &preset_p1);

        // A should still be toggled ON
        assert!(handler.button_active[0]);
    }

    #[test]
    fn encoder_still_works() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        handler.encoder_values[0] = 64;
        let r = handler.handle_events(&preset, &[InputEvent::Vol(Pulse::Clockwise)]);
        assert_eq!(r.midi.len(), 1);
        assert_eq!(r.midi[0].0, [0xB0, 7, 65]);
    }

    #[test]
    fn encoder_emits_display_event() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        handler.encoder_values[0] = 64;
        let r = handler.handle_events(&preset, &[InputEvent::Vol(Pulse::Clockwise)]);
        assert_eq!(r.display.len(), 1);
        match &r.display[0] {
            DisplayEvent::EncoderOverlay { side, label, value } => {
                assert_eq!(*side, DisplaySide::L);
                assert_eq!(label.as_str(), "Vol");
                assert_eq!(*value, 65);
            }
            _ => panic!("expected EncoderOverlay"),
        }
    }

    // --- Preset switch tests ---

    #[test]
    fn switch_preset_saves_and_restores_state() {
        // Use the LED preset which has button B as Toggle
        let preset = make_led_preset();
        let mut handler = PeHandler::new();

        // Toggle button B on
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        assert!(handler.button_active[1]);

        // Switch to preset 1 — fresh state
        handler.switch_preset(1, &preset, &preset);
        assert!(!handler.button_active[1]);

        // Switch back — toggle state restored
        handler.switch_preset(0, &preset, &preset);
        assert!(handler.button_active[1]);
    }

    #[test]
    fn switch_preset_recalls_encoder_values() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        handler.encoder_values[0] = 100;
        handler.switch_preset(1, &preset, &preset);
        assert_eq!(handler.encoder_values[0], 0);

        let recall = handler.switch_preset(0, &preset, &preset);
        assert_eq!(handler.encoder_values[0], 100);
        assert!(recall
            .iter()
            .any(|(raw, _)| raw[0] == 0xB0 && raw[1] == 7 && raw[2] == 100));
    }

    #[test]
    fn led_state_updates_after_switch() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        assert!(
            matches!(handler.led_state(&preset)[1], Animation::On(c) if c == RGB8::new(0, 0, 255))
        );

        handler.switch_preset(1, &preset, &preset);
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[1], Animation::On(c) if c == RGB8::new(255, 0, 0)));

        handler.switch_preset(0, &preset, &preset);
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[1], Animation::On(c) if c == RGB8::new(0, 0, 255)));
    }

    // --- LED state tests ---

    fn make_led_preset() -> Preset {
        let mut buttons: heapless::Vec<ButtonConfig, MAX_BUTTONS> = heapless::Vec::new();
        buttons
            .push(ButtonConfig {
                label: Label::new(),
                color: LedConfig {
                    on: Color::Green,
                    off: Color::Off,
                },
                mode: ButtonMode::Momentary,
                on_press: heapless::Vec::new(),
                on_release: heapless::Vec::new(),
                on_long_press: heapless::Vec::new(),
                cycle_values: heapless::Vec::new(),
            })
            .ok();
        buttons
            .push(ButtonConfig {
                label: Label::new(),
                color: LedConfig {
                    on: Color::Blue,
                    off: Color::Red,
                },
                mode: ButtonMode::Toggle,
                on_press: heapless::Vec::new(),
                on_release: heapless::Vec::new(),
                on_long_press: heapless::Vec::new(),
                cycle_values: heapless::Vec::new(),
            })
            .ok();
        Preset {
            name: Label::try_from("LED").unwrap(),
            buttons,
            encoders: heapless::Vec::new(),
            analog: heapless::Vec::new(),
        }
    }

    #[test]
    fn led_state_off_by_default() {
        let preset = make_led_preset();
        let handler = PeHandler::new();
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[0], Animation::On(c) if c == RGB8::new(0, 255/6, 0)));
        assert!(matches!(leds[1], Animation::On(c) if c == RGB8::new(255, 0, 0)));
    }

    #[test]
    fn led_state_momentary_on_while_pressed() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[0], Animation::On(c) if c == RGB8::new(0, 255, 0)));
    }

    #[test]
    fn led_state_momentary_off_after_release() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
        handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Deactivate)]);
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[0], Animation::On(c) if c == RGB8::new(0, 255/6, 0)));
    }

    #[test]
    fn led_state_toggle_alternates() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[1], Animation::On(c) if c == RGB8::new(0, 0, 255)));

        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[1], Animation::On(c) if c == RGB8::new(0, 0, 255)));

        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[1], Animation::On(c) if c == RGB8::new(255, 0, 0)));
    }

    #[test]
    fn led_state_encoder_heatmap() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        handler.encoder_values[0] = 64;
        handler.encoder_values[1] = 127;
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[6], Animation::Heatmap(6)));
        assert!(matches!(leds[7], Animation::Heatmap(12)));
    }

    #[test]
    fn led_dirty_on_button_press() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        let r = handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
        assert!(r.led_dirty);
    }

    #[test]
    fn led_not_dirty_when_idle() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        let r = handler.handle_events(&preset, &[]);
        assert!(!r.led_dirty);
    }
}

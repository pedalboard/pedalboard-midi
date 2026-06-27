//! PE preset event handler: processes input events against a PE preset config.

use crate::action::{action_to_midi, analog_cc, encoder_cc, EncoderDirection, MidiMessage};
use crate::events::{Edge, InputEvent, Pulse};
use crate::ledring::Animation;
use crate::long_press::{Gesture, LongPressDetector};
use pedalboard_protocol::config::{Action, ButtonMode, Color, Preset};
use smart_leds::RGB8;

const NUM_BUTTONS: usize = 6;

/// System-level actions that transcend MIDI output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAction {
    PresetNext,
    PresetPrev,
}

/// Result of processing events: MIDI messages + system actions + LED dirty flag.
pub struct HandleResult {
    pub midi: heapless::Vec<([u8; 3], usize), 8>,
    pub system: heapless::Vec<SystemAction, 2>,
    pub led_dirty: bool,
}

/// LED state for all 8 rings (A-F + Vol + Gain).
pub type LedAnimations = [Animation; 8];

/// Stateful PE event handler. Tracks encoder values, button state, and long-press.
pub struct PeHandler {
    pub encoder_values: [u8; 2],
    button_active: [bool; NUM_BUTTONS],
    long_press: [LongPressDetector; NUM_BUTTONS],
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
            long_press: core::array::from_fn(|_| LongPressDetector::new()),
        }
    }

    /// Returns true if any button is currently held (long-press counting).
    pub fn any_active(&self) -> bool {
        self.long_press.iter().any(|lp| lp.is_active())
    }

    /// Process input events against a PE preset. Returns MIDI messages and system actions.
    /// Call once per tick (1ms) — long press detection depends on tick rate.
    pub fn handle_events(&mut self, preset: &Preset, events: &[InputEvent]) -> HandleResult {
        let mut midi = heapless::Vec::new();
        let mut system = heapless::Vec::new();
        let mut led_dirty = false;

        // Update long-press detectors for all buttons
        for i in 0..NUM_BUTTONS {
            let edge = events.iter().find_map(|e| match (e, i) {
                (InputEvent::ButtonA(e), 0) => Some(*e),
                (InputEvent::ButtonB(e), 1) => Some(*e),
                (InputEvent::ButtonC(e), 2) => Some(*e),
                (InputEvent::ButtonD(e), 3) => Some(*e),
                (InputEvent::ButtonE(e), 4) => Some(*e),
                (InputEvent::ButtonF(e), 5) => Some(*e),
                _ => None,
            });

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
                // Track physical press for LED (momentary visual feedback)
                if !matches!(mode, &ButtonMode::Toggle) {
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
                match self.long_press[i].update(edge) {
                    Some(Gesture::ShortPress) => {
                        if let Some(btn) = preset.buttons.get(i) {
                            if mode == &ButtonMode::Toggle {
                                self.button_active[i] = !self.button_active[i];
                                led_dirty = true;
                            }
                            execute_actions(&btn.on_press, &mut midi, &mut system);
                            execute_actions(&btn.on_release, &mut midi, &mut system);
                        }
                    }
                    Some(Gesture::LongPress) => {
                        if let Some(btn) = preset.buttons.get(i) {
                            execute_actions(&btn.on_long_press, &mut midi, &mut system);
                        }
                    }
                    None => {}
                }
            } else {
                match edge {
                    Some(Edge::Activate) => {
                        if let Some(btn) = preset.buttons.get(i) {
                            match mode {
                                ButtonMode::Toggle => {
                                    self.button_active[i] = !self.button_active[i];
                                    led_dirty = true;
                                }
                                ButtonMode::Momentary => {
                                    self.button_active[i] = true;
                                    led_dirty = true;
                                }
                                _ => {
                                    self.button_active[i] = true;
                                    led_dirty = true;
                                }
                            }
                            execute_actions(&btn.on_press, &mut midi, &mut system);
                        }
                    }
                    Some(Edge::Deactivate) => {
                        if let Some(btn) = preset.buttons.get(i) {
                            if !matches!(mode, ButtonMode::Toggle) {
                                self.button_active[i] = false;
                                led_dirty = true;
                            }
                            execute_actions(&btn.on_release, &mut midi, &mut system);
                        }
                    }
                    None => {}
                }
            }
        }

        // Encoder/analog events also dirty LEDs (heatmap updates)
        for event in events {
            match event {
                InputEvent::Vol(pulse) => {
                    let dir = pulse_to_dir(*pulse);
                    if let Some(msg) = encoder_cc(preset, 0, dir, &mut self.encoder_values[0]) {
                        push_midi(&mut midi, &msg);
                        led_dirty = true;
                    }
                }
                InputEvent::Gain(pulse) => {
                    let dir = pulse_to_dir(*pulse);
                    if let Some(msg) = encoder_cc(preset, 1, dir, &mut self.encoder_values[1]) {
                        push_midi(&mut midi, &msg);
                        led_dirty = true;
                    }
                }
                InputEvent::ExpressionPedalA(raw_adc) => {
                    if let Some(msg) = analog_cc(preset, 0, *raw_adc, 4095) {
                        push_midi(&mut midi, &msg);
                    }
                }
                InputEvent::ExpressionPedalB(raw_adc) => {
                    if let Some(msg) = analog_cc(preset, 1, *raw_adc, 4095) {
                        push_midi(&mut midi, &msg);
                    }
                }
                _ => {}
            }
        }
        HandleResult {
            midi,
            system,
            led_dirty,
        }
    }

    /// Compute LED animations for all 8 rings based on current state + preset config.
    /// Order: [A, B, C, D, E, F, Vol, Gain]
    pub fn led_state(&self, preset: &Preset) -> LedAnimations {
        let mut anims = [Animation::Off; 8];

        // Button rings A-F
        for (i, anim) in anims.iter_mut().enumerate().take(NUM_BUTTONS) {
            if let Some(btn) = preset.buttons.get(i) {
                let color = if self.button_active[i] {
                    color_to_rgb(&btn.color.on)
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

        // Encoder rings: heatmap from current value
        let fill_vol = ((self.encoder_values[0] as u16 * 12) / 127).min(12) as u8;
        anims[6] = Animation::Heatmap(fill_vol);
        let fill_gain = ((self.encoder_values[1] as u16 * 12) / 127).min(12) as u8;
        anims[7] = Animation::Heatmap(fill_gain);

        anims
    }
}

fn execute_actions(
    actions: &heapless::Vec<Action, { pedalboard_protocol::config::MAX_ACTIONS }>,
    midi: &mut heapless::Vec<([u8; 3], usize), 8>,
    system: &mut heapless::Vec<SystemAction, 2>,
) {
    for action in actions {
        match action {
            Action::PresetNext => {
                system.push(SystemAction::PresetNext).ok();
            }
            Action::PresetPrev => {
                system.push(SystemAction::PresetPrev).ok();
            }
            _ => {
                if let Some(msg) = action_to_midi(action) {
                    push_midi(midi, &msg);
                }
            }
        }
    }
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

fn push_midi(messages: &mut heapless::Vec<([u8; 3], usize), 8>, msg: &MidiMessage) {
    let mut raw = [0u8; 3];
    let len = msg.len.min(3);
    raw[..len].copy_from_slice(&msg.data[..len]);
    messages.push((raw, len)).ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use heapless::Vec;
    use pedalboard_protocol::config::*;

    fn make_test_preset() -> Preset {
        let mut buttons: Vec<ButtonConfig, MAX_BUTTONS> = Vec::new();
        // Button A: on_press + on_release, no long_press
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
            })
            .ok();

        // Button B: has on_long_press
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
        let events = [InputEvent::ButtonA(Edge::Activate)];
        let r = handler.handle_events(&preset, &events);
        assert_eq!(r.midi.len(), 1);
        assert_eq!(r.midi[0].0, [0x90, 60, 127]); // NoteOn
    }

    #[test]
    fn on_release_fires_on_deactivate() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        let events = [InputEvent::ButtonA(Edge::Deactivate)];
        let r = handler.handle_events(&preset, &events);
        assert_eq!(r.midi.len(), 1);
        assert_eq!(r.midi[0].0, [0x80, 60, 0]); // NoteOff
    }

    #[test]
    fn long_press_button_defers_on_press() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        // Press button B (has long_press) — should NOT fire immediately
        let events = [InputEvent::ButtonB(Edge::Activate)];
        let r = handler.handle_events(&preset, &events);
        assert!(r.midi.is_empty());
    }

    #[test]
    fn long_press_short_release_fires_on_press() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        // Press
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        // Tick a few times (not enough for long press)
        for _ in 0..100 {
            handler.handle_events(&preset, &[]);
        }
        // Release → short press fires on_press
        let r = handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
        assert_eq!(r.midi.len(), 1);
        assert_eq!(r.midi[0].0, [0xB0, 10, 127]); // CC
    }

    #[test]
    fn long_press_fires_on_long_press() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        // Press
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        // Tick past threshold (500ms)
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
        // Press, hold past threshold
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        for _ in 0..501 {
            handler.handle_events(&preset, &[]);
        }
        // Release after long press — should NOT fire on_press
        let r = handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
        assert!(r.midi.is_empty());
    }

    #[test]
    fn encoder_still_works() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        handler.encoder_values[0] = 64;
        let events = [InputEvent::Vol(Pulse::Clockwise)];
        let r = handler.handle_events(&preset, &events);
        assert_eq!(r.midi.len(), 1);
        assert_eq!(r.midi[0].0, [0xB0, 7, 65]);
    }

    // --- LED state tests ---

    fn make_led_preset() -> Preset {
        use pedalboard_protocol::config::*;
        let mut buttons: heapless::Vec<ButtonConfig, MAX_BUTTONS> = heapless::Vec::new();
        // Button A: momentary, green on, off when inactive
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
            })
            .ok();
        // Button B: toggle, blue on, red off
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
        // Button A: off color = Off → Animation::Off
        assert!(matches!(leds[0], Animation::Off));
        // Button B: off color = Red → Animation::On(red)
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
        assert!(matches!(leds[0], Animation::Off));
    }

    #[test]
    fn led_state_toggle_alternates() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        // First press → toggle on (blue)
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[1], Animation::On(c) if c == RGB8::new(0, 0, 255)));
        // Release doesn't change toggle state
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[1], Animation::On(c) if c == RGB8::new(0, 0, 255)));
        // Second press → toggle off (red)
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
        // Vol ring at ~midpoint
        assert!(matches!(leds[6], Animation::Heatmap(6)));
        // Gain ring at max
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

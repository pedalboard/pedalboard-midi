//! PE preset event handler: processes input events against a PE preset config.

use crate::action::{action_to_midi, analog_cc, encoder_cc, EncoderDirection, MidiMessage};
use crate::events::{Edge, InputEvent, Pulse};
use crate::long_press::{Gesture, LongPressDetector};
use pedalboard_protocol::config::{Action, Preset};

const NUM_BUTTONS: usize = 6;

/// System-level actions that transcend MIDI output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAction {
    PresetNext,
    PresetPrev,
}

/// Result of processing events: MIDI messages + system actions.
pub struct HandleResult {
    pub midi: heapless::Vec<([u8; 3], usize), 8>,
    pub system: heapless::Vec<SystemAction, 2>,
}

/// Stateful PE event handler. Tracks encoder values and long-press state.
pub struct PeHandler {
    pub encoder_values: [u8; 2],
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

            if has_long_press {
                match self.long_press[i].update(edge) {
                    Some(Gesture::ShortPress) => {
                        if let Some(btn) = preset.buttons.get(i) {
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
                            execute_actions(&btn.on_press, &mut midi, &mut system);
                        }
                    }
                    Some(Edge::Deactivate) => {
                        if let Some(btn) = preset.buttons.get(i) {
                            execute_actions(&btn.on_release, &mut midi, &mut system);
                        }
                    }
                    None => {}
                }
            }
        }

        // Handle encoders and analog
        for event in events {
            match event {
                InputEvent::Vol(pulse) => {
                    let dir = pulse_to_dir(*pulse);
                    if let Some(msg) = encoder_cc(preset, 0, dir, &mut self.encoder_values[0]) {
                        push_midi(&mut midi, &msg);
                    }
                }
                InputEvent::Gain(pulse) => {
                    let dir = pulse_to_dir(*pulse);
                    if let Some(msg) = encoder_cc(preset, 1, dir, &mut self.encoder_values[1]) {
                        push_midi(&mut midi, &msg);
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
        HandleResult { midi, system }
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
}

//! PE preset event handler: processes input events against a PE preset config.

use crate::action::{analog_cc, encoder_cc, execute_button_press, EncoderDirection, MidiMessage};
use crate::events::{Edge, InputEvent, Pulse};
use pedalboard_protocol::config::Preset;

/// Process input events against a PE preset. Returns raw MIDI messages to send.
/// `encoder_values` tracks absolute encoder positions across calls.
pub fn handle_events(
    preset: &Preset,
    events: &[InputEvent],
    encoder_values: &mut [u8; 2],
) -> heapless::Vec<([u8; 3], usize), 8> {
    let mut messages = heapless::Vec::new();

    for event in events {
        match event {
            InputEvent::ButtonA(e)
            | InputEvent::ButtonB(e)
            | InputEvent::ButtonC(e)
            | InputEvent::ButtonD(e)
            | InputEvent::ButtonE(e)
            | InputEvent::ButtonF(e) => {
                if *e != Edge::Activate {
                    continue;
                }
                let btn_idx = match event {
                    InputEvent::ButtonA(_) => 0,
                    InputEvent::ButtonB(_) => 1,
                    InputEvent::ButtonC(_) => 2,
                    InputEvent::ButtonD(_) => 3,
                    InputEvent::ButtonE(_) => 4,
                    InputEvent::ButtonF(_) => 5,
                    _ => continue,
                };
                for msg in execute_button_press(preset, btn_idx) {
                    push_midi(&mut messages, &msg);
                }
            }
            InputEvent::Vol(pulse) => {
                let dir = pulse_to_dir(*pulse);
                if let Some(msg) = encoder_cc(preset, 0, dir, &mut encoder_values[0]) {
                    push_midi(&mut messages, &msg);
                }
            }
            InputEvent::Gain(pulse) => {
                let dir = pulse_to_dir(*pulse);
                if let Some(msg) = encoder_cc(preset, 1, dir, &mut encoder_values[1]) {
                    push_midi(&mut messages, &msg);
                }
            }
            InputEvent::ExpressionPedalA(raw_adc) => {
                if let Some(msg) = analog_cc(preset, 0, *raw_adc, 4095) {
                    push_midi(&mut messages, &msg);
                }
            }
            InputEvent::ExpressionPedalB(raw_adc) => {
                if let Some(msg) = analog_cc(preset, 1, *raw_adc, 4095) {
                    push_midi(&mut messages, &msg);
                }
            }
            InputEvent::VolButton(_) | InputEvent::GainButton(_) => {}
        }
    }
    messages
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
        let mut on_press: Vec<Action, MAX_ACTIONS> = Vec::new();
        on_press
            .push(Action::Cc {
                cc: 10,
                value: 127,
                channel: 1,
            })
            .ok();
        on_press
            .push(Action::ProgramChange {
                program: 5,
                channel: 2,
            })
            .ok();
        buttons
            .push(ButtonConfig {
                label: Label::new(),
                color: LedConfig::default(),
                mode: ButtonMode::default(),
                on_press,
                on_release: Vec::new(),
                on_long_press: Vec::new(),
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
        encoders
            .push(EncoderConfig {
                label: Label::try_from("Gain").unwrap(),
                action: EncoderAction::Cc {
                    cc: 91,
                    channel: 1,
                    min: 0,
                    max: 127,
                },
            })
            .ok();

        let mut analog: Vec<AnalogConfig, MAX_ANALOG> = Vec::new();
        analog
            .push(AnalogConfig {
                label: Label::try_from("Exp").unwrap(),
                cc: 11,
                channel: 1,
                min: 0,
                max: 127,
            })
            .ok();

        Preset {
            name: Label::try_from("Test").unwrap(),
            buttons,
            encoders,
            analog,
        }
    }

    #[test]
    fn button_press_generates_midi() {
        let preset = make_test_preset();
        let events = [InputEvent::ButtonA(Edge::Activate)];
        let mut enc = [0u8; 2];
        let msgs = handle_events(&preset, &events, &mut enc);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].0[..3], [0xB0, 10, 127]);
        assert_eq!(msgs[1].0[..2], [0xC1, 5]);
    }

    #[test]
    fn button_release_ignored() {
        let preset = make_test_preset();
        let events = [InputEvent::ButtonA(Edge::Deactivate)];
        let mut enc = [0u8; 2];
        let msgs = handle_events(&preset, &events, &mut enc);
        assert!(msgs.is_empty());
    }

    #[test]
    fn encoder_generates_cc() {
        let preset = make_test_preset();
        let events = [
            InputEvent::Vol(Pulse::Clockwise),
            InputEvent::Gain(Pulse::CounterClockwise),
        ];
        let mut enc = [64u8, 64];
        let msgs = handle_events(&preset, &events, &mut enc);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].0, [0xB0, 7, 65]); // Vol CW
        assert_eq!(msgs[1].0, [0xB0, 91, 63]); // Gain CCW
        assert_eq!(enc, [65, 63]);
    }

    #[test]
    fn analog_generates_cc() {
        let preset = make_test_preset();
        let events = [InputEvent::ExpressionPedalA(2048)];
        let mut enc = [0u8; 2];
        let msgs = handle_events(&preset, &events, &mut enc);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].0[0], 0xB0);
        assert_eq!(msgs[0].0[1], 11);
        assert!(msgs[0].0[2] >= 63); // ~midpoint
    }

    #[test]
    fn unconfigured_encoder_silent() {
        let preset = Preset {
            name: Label::try_from("Empty").unwrap(),
            buttons: Vec::new(),
            encoders: Vec::new(),
            analog: Vec::new(),
        };
        let events = [InputEvent::Vol(Pulse::Clockwise)];
        let mut enc = [0u8; 2];
        let msgs = handle_events(&preset, &events, &mut enc);
        assert!(msgs.is_empty());
    }

    #[test]
    fn mixed_events() {
        let preset = make_test_preset();
        let events = [
            InputEvent::ButtonA(Edge::Activate),
            InputEvent::Vol(Pulse::Clockwise),
            InputEvent::ExpressionPedalA(4095),
        ];
        let mut enc = [0u8; 2];
        let msgs = handle_events(&preset, &events, &mut enc);
        // 2 button actions + 1 encoder + 1 analog = 4
        assert_eq!(msgs.len(), 4);
    }
}

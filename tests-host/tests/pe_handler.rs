// Host-side tests for src/pe_handler.rs

#[path = "../../src/events.rs"]
mod events;

#[path = "../../src/action.rs"]
mod action;

#[path = "../../src/pe_handler.rs"]
mod pe_handler;

use events::{Edge, InputEvent, Pulse};
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
    let msgs = pe_handler::handle_events(&preset, &events, &mut enc);
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].0, [0xB0, 10, 127]);
    assert_eq!(msgs[1].0[..2], [0xC1, 5]);
}

#[test]
fn button_release_ignored() {
    let preset = make_test_preset();
    let events = [InputEvent::ButtonA(Edge::Deactivate)];
    let mut enc = [0u8; 2];
    let msgs = pe_handler::handle_events(&preset, &events, &mut enc);
    assert!(msgs.is_empty());
}

#[test]
fn encoder_vol_clockwise() {
    let preset = make_test_preset();
    let events = [InputEvent::Vol(Pulse::Clockwise)];
    let mut enc = [64u8, 0];
    let msgs = pe_handler::handle_events(&preset, &events, &mut enc);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].0, [0xB0, 7, 65]);
    assert_eq!(enc[0], 65);
}

#[test]
fn encoder_gain_counter_clockwise() {
    let preset = make_test_preset();
    let events = [InputEvent::Gain(Pulse::CounterClockwise)];
    let mut enc = [0, 64u8];
    let msgs = pe_handler::handle_events(&preset, &events, &mut enc);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].0, [0xB0, 91, 63]);
    assert_eq!(enc[1], 63);
}

#[test]
fn analog_expression_pedal() {
    let preset = make_test_preset();
    let events = [InputEvent::ExpressionPedalA(4095)];
    let mut enc = [0u8; 2];
    let msgs = pe_handler::handle_events(&preset, &events, &mut enc);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].0, [0xB0, 11, 127]);
}

#[test]
fn empty_preset_produces_no_output() {
    let preset = Preset {
        name: Label::try_from("Empty").unwrap(),
        buttons: Vec::new(),
        encoders: Vec::new(),
        analog: Vec::new(),
    };
    let events = [
        InputEvent::ButtonA(Edge::Activate),
        InputEvent::Vol(Pulse::Clockwise),
        InputEvent::ExpressionPedalA(2048),
    ];
    let mut enc = [0u8; 2];
    let msgs = pe_handler::handle_events(&preset, &events, &mut enc);
    assert!(msgs.is_empty());
}

#[test]
fn mixed_events_all_generate() {
    let preset = make_test_preset();
    let events = [
        InputEvent::ButtonA(Edge::Activate),
        InputEvent::Vol(Pulse::Clockwise),
        InputEvent::ExpressionPedalA(4095),
    ];
    let mut enc = [0u8; 2];
    let msgs = pe_handler::handle_events(&preset, &events, &mut enc);
    // 2 button actions + 1 encoder + 1 analog = 4
    assert_eq!(msgs.len(), 4);
}

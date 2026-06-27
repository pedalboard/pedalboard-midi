// Host-side tests for src/pe_handler.rs

#[path = "../../src/events.rs"]
mod events;

#[path = "../../src/action.rs"]
mod action;

#[path = "../../src/long_press.rs"]
mod long_press;

#[path = "../../src/pe_handler.rs"]
mod pe_handler;

use events::{Edge, InputEvent, Pulse};
use heapless::Vec;
use pe_handler::PeHandler;
use pedalboard_protocol::config::*;

fn make_test_preset() -> Preset {
    let mut buttons: Vec<ButtonConfig, MAX_BUTTONS> = Vec::new();
    // Button A: on_press NoteOn + on_release NoteOff (no long_press)
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

    // Button B: has on_long_press (deferred press)
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
fn on_press_fires_immediately_without_long_press() {
    let preset = make_test_preset();
    let mut handler = PeHandler::new();
    let msgs = handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].0, [0x90, 60, 127]);
}

#[test]
fn on_release_fires_note_off() {
    let preset = make_test_preset();
    let mut handler = PeHandler::new();
    let msgs = handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Deactivate)]);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].0, [0x80, 60, 0]);
}

#[test]
fn long_press_button_defers_on_press() {
    let preset = make_test_preset();
    let mut handler = PeHandler::new();
    // Press B (has long_press) — should NOT fire immediately
    let msgs = handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
    assert!(msgs.is_empty());
}

#[test]
fn long_press_short_release_fires_on_press() {
    let preset = make_test_preset();
    let mut handler = PeHandler::new();
    handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
    for _ in 0..100 {
        handler.handle_events(&preset, &[]);
    }
    // Release before threshold → fires on_press
    let msgs = handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].0, [0xB0, 10, 127]);
}

#[test]
fn long_press_fires_on_long_press_action() {
    let preset = make_test_preset();
    let mut handler = PeHandler::new();
    handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
    let mut found = false;
    for _ in 0..501 {
        let msgs = handler.handle_events(&preset, &[]);
        if !msgs.is_empty() {
            assert_eq!(msgs[0].0[..2], [0xC0, 5]); // ProgramChange
            found = true;
            break;
        }
    }
    assert!(found);
}

#[test]
fn long_press_suppresses_on_press_at_release() {
    let preset = make_test_preset();
    let mut handler = PeHandler::new();
    handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
    for _ in 0..501 {
        handler.handle_events(&preset, &[]);
    }
    // Release after long press — no on_press
    let msgs = handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
    assert!(msgs.is_empty());
}

#[test]
fn encoder_generates_cc() {
    let preset = make_test_preset();
    let mut handler = PeHandler::new();
    handler.encoder_values[0] = 64;
    let msgs = handler.handle_events(&preset, &[InputEvent::Vol(Pulse::Clockwise)]);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].0, [0xB0, 7, 65]);
}

#[test]
fn analog_generates_cc() {
    let preset = make_test_preset();
    let mut handler = PeHandler::new();
    let msgs = handler.handle_events(&preset, &[InputEvent::ExpressionPedalA(4095)]);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].0, [0xB0, 11, 127]);
}

#[test]
fn empty_preset_is_silent() {
    let preset = Preset {
        name: Label::try_from("Empty").unwrap(),
        buttons: Vec::new(),
        encoders: Vec::new(),
        analog: Vec::new(),
    };
    let mut handler = PeHandler::new();
    let msgs = handler.handle_events(
        &preset,
        &[
            InputEvent::ButtonA(Edge::Activate),
            InputEvent::Vol(Pulse::Clockwise),
        ],
    );
    assert!(msgs.is_empty());
}

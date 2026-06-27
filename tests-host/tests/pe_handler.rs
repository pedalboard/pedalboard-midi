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
use pe_handler::{PeHandler, SystemAction};
use pedalboard_protocol::config::*;

fn make_test_preset() -> Preset {
    let mut buttons: Vec<ButtonConfig, MAX_BUTTONS> = Vec::new();
    // Button A: NoteOn/NoteOff (no long_press)
    buttons
        .push(ButtonConfig {
            label: Label::new(),
            color: LedConfig::default(),
            mode: ButtonMode::default(),
            on_press: {
                let mut v = Vec::new();
                v.push(Action::NoteOn { note: 60, channel: 1 }).ok();
                v
            },
            on_release: {
                let mut v = Vec::new();
                v.push(Action::NoteOff { note: 60, channel: 1 }).ok();
                v
            },
            on_long_press: Vec::new(),
        })
        .ok();

    // Button B: CC on press, PresetNext on long_press
    buttons
        .push(ButtonConfig {
            label: Label::new(),
            color: LedConfig::default(),
            mode: ButtonMode::default(),
            on_press: {
                let mut v = Vec::new();
                v.push(Action::Cc { cc: 10, value: 127, channel: 1 }).ok();
                v
            },
            on_release: Vec::new(),
            on_long_press: {
                let mut v = Vec::new();
                v.push(Action::PresetNext).ok();
                v
            },
        })
        .ok();

    // Button C: PresetPrev on long_press
    buttons
        .push(ButtonConfig {
            label: Label::new(),
            color: LedConfig::default(),
            mode: ButtonMode::default(),
            on_press: {
                let mut v = Vec::new();
                v.push(Action::Cc { cc: 11, value: 127, channel: 1 }).ok();
                v
            },
            on_release: Vec::new(),
            on_long_press: {
                let mut v = Vec::new();
                v.push(Action::PresetPrev).ok();
                v
            },
        })
        .ok();

    let mut encoders: Vec<EncoderConfig, MAX_ENCODERS> = Vec::new();
    encoders
        .push(EncoderConfig {
            label: Label::try_from("Vol").unwrap(),
            action: EncoderAction::Cc { cc: 7, channel: 1, min: 0, max: 127 },
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
fn on_press_fires_immediately() {
    let preset = make_test_preset();
    let mut h = PeHandler::new();
    let r = h.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
    assert_eq!(r.midi.len(), 1);
    assert_eq!(r.midi[0].0, [0x90, 60, 127]);
    assert!(r.system.is_empty());
}

#[test]
fn on_release_fires_note_off() {
    let preset = make_test_preset();
    let mut h = PeHandler::new();
    let r = h.handle_events(&preset, &[InputEvent::ButtonA(Edge::Deactivate)]);
    assert_eq!(r.midi.len(), 1);
    assert_eq!(r.midi[0].0, [0x80, 60, 0]);
}

#[test]
fn long_press_defers_on_press() {
    let preset = make_test_preset();
    let mut h = PeHandler::new();
    let r = h.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
    assert!(r.midi.is_empty());
    assert!(r.system.is_empty());
}

#[test]
fn short_release_fires_on_press() {
    let preset = make_test_preset();
    let mut h = PeHandler::new();
    h.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
    for _ in 0..100 {
        h.handle_events(&preset, &[]);
    }
    let r = h.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
    assert_eq!(r.midi.len(), 1);
    assert_eq!(r.midi[0].0, [0xB0, 10, 127]);
    assert!(r.system.is_empty());
}

#[test]
fn long_press_fires_preset_next() {
    let preset = make_test_preset();
    let mut h = PeHandler::new();
    h.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
    let mut found = false;
    for _ in 0..501 {
        let r = h.handle_events(&preset, &[]);
        if !r.system.is_empty() {
            assert_eq!(r.system[0], SystemAction::PresetNext);
            assert!(r.midi.is_empty());
            found = true;
            break;
        }
    }
    assert!(found);
}

#[test]
fn long_press_fires_preset_prev() {
    let preset = make_test_preset();
    let mut h = PeHandler::new();
    h.handle_events(&preset, &[InputEvent::ButtonC(Edge::Activate)]);
    let mut found = false;
    for _ in 0..501 {
        let r = h.handle_events(&preset, &[]);
        if !r.system.is_empty() {
            assert_eq!(r.system[0], SystemAction::PresetPrev);
            found = true;
            break;
        }
    }
    assert!(found);
}

#[test]
fn long_press_suppresses_on_press() {
    let preset = make_test_preset();
    let mut h = PeHandler::new();
    h.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
    for _ in 0..501 {
        h.handle_events(&preset, &[]);
    }
    let r = h.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
    assert!(r.midi.is_empty());
    assert!(r.system.is_empty());
}

#[test]
fn encoder_generates_cc() {
    let preset = make_test_preset();
    let mut h = PeHandler::new();
    h.encoder_values[0] = 64;
    let r = h.handle_events(&preset, &[InputEvent::Vol(Pulse::Clockwise)]);
    assert_eq!(r.midi.len(), 1);
    assert_eq!(r.midi[0].0, [0xB0, 7, 65]);
}

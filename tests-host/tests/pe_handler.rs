// Host-side tests for src/pe_handler.rs

#[path = "../../src/events.rs"]
mod events;

#[path = "../../src/action.rs"]
mod action;

#[path = "../../src/ledring.rs"]
mod ledring;

#[path = "../../src/pe_handler.rs"]
mod pe_handler;

use events::{Edge, InputEvent, Pulse};
use heapless::Vec;
use pe_handler::{MidiStep, PeHandler};
use pedalboard_protocol::config::*;

fn make_config() -> Config {
    let mut presets: Vec<Preset, MAX_PRESETS> = Vec::new();
    presets.push(make_test_preset()).ok();
    // Add a second preset so preset switching works
    presets.push(make_test_preset()).ok();
    Config { presets }
}

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
                v.push(Action::note_on(60, 1).unwrap()).ok();
                v
            },
            on_release: {
                let mut v = Vec::new();
                v.push(Action::note_off(60, 1).unwrap()).ok();
                v
            },
            on_long_press: Vec::new(),
            cycle_values: Vec::new(),
            listen_cc: None,
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
                v.push(Action::cc(10, 127, 1).unwrap()).ok();
                v
            },
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

    // Button C: PresetPrev on long_press
    buttons
        .push(ButtonConfig {
            label: Label::new(),
            color: LedConfig::default(),
            mode: ButtonMode::default(),
            on_press: {
                let mut v = Vec::new();
                v.push(Action::cc(11, 127, 1).unwrap()).ok();
                v
            },
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
        defaults: Default::default(),
        on_enter: heapless::Vec::new(),
        on_exit: heapless::Vec::new(),
        triggers: heapless::Vec::new(),
    }
}

#[test]
fn on_press_fires_immediately() {
    let config = make_config();
    let mut h = PeHandler::new();
    let cal = pe_handler::AdcCalibration::default();
    let r = h.handle_events(&config, &[InputEvent::ButtonA(Edge::Activate)], &cal, 0);
    assert_eq!(r.midi.len(), 1);
    assert!(matches!(&r.midi[0], MidiStep::Send(d, _) if *d == [0x90, 60, 127]));
}

#[test]
fn on_release_fires_note_off() {
    let config = make_config();
    let mut h = PeHandler::new();
    let cal = pe_handler::AdcCalibration::default();
    let r = h.handle_events(&config, &[InputEvent::ButtonA(Edge::Deactivate)], &cal, 0);
    assert_eq!(r.midi.len(), 1);
    assert!(matches!(&r.midi[0], MidiStep::Send(d, _) if *d == [0x80, 60, 0]));
}

#[test]
fn long_press_defers_on_press() {
    let config = make_config();
    let mut h = PeHandler::new();
    let cal = pe_handler::AdcCalibration::default();
    let r = h.handle_events(&config, &[InputEvent::ButtonB(Edge::Activate)], &cal, 0);
    assert!(r.midi.is_empty());
}

#[test]
fn short_release_fires_on_press() {
    let config = make_config();
    let mut h = PeHandler::new();
    let cal = pe_handler::AdcCalibration::default();
    h.handle_events(&config, &[InputEvent::ButtonB(Edge::Activate)], &cal, 0);
    for i in 1..=100u32 {
        h.handle_events(&config, &[], &cal, i);
    }
    let r = h.handle_events(
        &config,
        &[InputEvent::ButtonB(Edge::Deactivate)],
        &cal,
        101,
    );
    assert_eq!(r.midi.len(), 1);
    assert!(matches!(&r.midi[0], MidiStep::Send(d, _) if *d == [0xB0, 10, 127]));
}

#[test]
fn long_press_switches_preset() {
    let config = make_config();
    let mut h = PeHandler::new();
    let cal = pe_handler::AdcCalibration::default();
    h.handle_events(&config, &[InputEvent::ButtonB(Edge::Activate)], &cal, 0);
    let mut switched = false;
    for i in 1..=501u32 {
        let r = h.handle_events(&config, &[], &cal, i);
        if r.preset_changed {
            assert_eq!(h.active_preset(), 1);
            switched = true;
            break;
        }
    }
    assert!(switched);
}

#[test]
fn long_press_prev_switches_preset() {
    let config = make_config();
    let mut h = PeHandler::new();
    let cal = pe_handler::AdcCalibration::default();
    h.handle_events(&config, &[InputEvent::ButtonC(Edge::Activate)], &cal, 0);
    let mut switched = false;
    for i in 1..=501u32 {
        let r = h.handle_events(&config, &[], &cal, i);
        if r.preset_changed {
            // From preset 0, prev wraps to preset 1 (2 presets total)
            assert_eq!(h.active_preset(), 1);
            switched = true;
            break;
        }
    }
    assert!(switched);
}

#[test]
fn long_press_suppresses_on_press() {
    let config = make_config();
    let mut h = PeHandler::new();
    let cal = pe_handler::AdcCalibration::default();
    h.handle_events(&config, &[InputEvent::ButtonB(Edge::Activate)], &cal, 0);
    for i in 1..=501u32 {
        h.handle_events(&config, &[], &cal, i);
    }
    let r = h.handle_events(
        &config,
        &[InputEvent::ButtonB(Edge::Deactivate)],
        &cal,
        502,
    );
    // After long press fired (and preset switched), release should produce nothing
    assert!(r.midi.is_empty());
}

#[test]
fn encoder_generates_cc() {
    let config = make_config();
    let mut h = PeHandler::new();
    let cal = pe_handler::AdcCalibration::default();
    h.set_encoder_value(0, 64);
    let r = h.handle_events(&config, &[InputEvent::Vol(Pulse::Clockwise)], &cal, 0);
    assert_eq!(r.midi.len(), 1);
    assert!(matches!(&r.midi[0], MidiStep::Send(d, _) if *d == [0xB0, 7, 65]));
}

#[test]
fn action_sequence_with_delay() {
    let mut buttons: Vec<ButtonConfig, MAX_BUTTONS> = Vec::new();
    buttons
        .push(ButtonConfig {
            label: Label::new(),
            color: LedConfig::default(),
            mode: ButtonMode::default(),
            on_press: {
                let mut v = Vec::new();
                v.push(Action::cc(1, 127, 1).unwrap()).ok();
                v.push(Action::Delay(100)).ok();
                v.push(Action::cc(1, 0, 1).unwrap()).ok();
                v
            },
            on_release: Vec::new(),
            on_long_press: Vec::new(),
            cycle_values: Vec::new(),
            listen_cc: None,
        })
        .ok();
    let preset = Preset {
        name: Label::try_from("Delay").unwrap(),
        buttons,
        encoders: Vec::new(),
        analog: Vec::new(),
        defaults: Default::default(),
        on_enter: heapless::Vec::new(),
        on_exit: heapless::Vec::new(),
        triggers: heapless::Vec::new(),
    };
    let mut presets: Vec<Preset, MAX_PRESETS> = Vec::new();
    presets.push(preset).ok();
    let config = Config { presets };

    let mut h = PeHandler::new();
    let cal = pe_handler::AdcCalibration::default();
    let r = h.handle_events(&config, &[InputEvent::ButtonA(Edge::Activate)], &cal, 0);
    assert_eq!(r.midi.len(), 3);
    assert!(matches!(&r.midi[0], MidiStep::Send(d, _) if *d == [0xB0, 1, 127]));
    assert_eq!(r.midi[1], MidiStep::Delay(100));
    assert!(matches!(&r.midi[2], MidiStep::Send(d, _) if *d == [0xB0, 1, 0]));
}

// Host-side tests for src/config_mode.rs

#[path = "../../src/events.rs"]
mod events;

#[path = "../../src/config_mode.rs"]
mod config_mode;

use config_mode::{ButtonAction, ConfigContext, ConfigDisplayEvent, ConfigMode};
use events::{Edge, InputEvent, Pulse};

fn test_context() -> ConfigContext<'static> {
    static ACTIONS: [ButtonAction; 6] = [
        ButtonAction {
            summary: heapless::String::new(),
        },
        ButtonAction {
            summary: heapless::String::new(),
        },
        ButtonAction {
            summary: heapless::String::new(),
        },
        ButtonAction {
            summary: heapless::String::new(),
        },
        ButtonAction {
            summary: heapless::String::new(),
        },
        ButtonAction {
            summary: heapless::String::new(),
        },
    ];
    ConfigContext {
        firmware_version: "0.2.0",
        git_hash: "abc1234",
        preset_count: 5,
        din_enabled: true,
        midi_clock: false,
        bpm: 120,
        din_to_usb_thru: true,
        usb_to_din_thru: false,
        usb_to_usb_thru: false,
        button_actions: &ACTIONS,
        encoder_configs: [config_mode::EncoderInfo::default(), config_mode::EncoderInfo::default()],
        analog_configs: [config_mode::AnalogInfo::default(), config_mode::AnalogInfo::default()],
    }
}

#[test]
fn entry_requires_both_encoder_buttons_held() {
    let mut cm = ConfigMode::new();
    let ctx = test_context();

    // Only Vol held — not enough.
    let events = [InputEvent::VolButton(Edge::Activate)];
    cm.process_events(&events, 0, &ctx);
    cm.process_events(&[], 1500, &ctx);
    assert!(!cm.is_active());
}

#[test]
fn entry_after_hold_duration() {
    let mut cm = ConfigMode::new();
    let ctx = test_context();

    let events = [
        InputEvent::VolButton(Edge::Activate),
        InputEvent::GainButton(Edge::Activate),
    ];
    cm.process_events(&events, 0, &ctx);
    assert!(!cm.is_active());

    // At 999ms — not yet.
    cm.process_events(&[], 999, &ctx);
    assert!(!cm.is_active());

    // At 1000ms — enters.
    let result = cm.process_events(&[], 1000, &ctx);
    assert!(cm.is_active());
    assert!(result
        .iter()
        .any(|e| matches!(e, ConfigDisplayEvent::Entered)));
}

#[test]
fn exit_after_release_and_re_hold() {
    let mut cm = ConfigMode::new();
    let ctx = test_context();

    // Enter.
    let events = [
        InputEvent::VolButton(Edge::Activate),
        InputEvent::GainButton(Edge::Activate),
    ];
    cm.process_events(&events, 0, &ctx);
    cm.process_events(&[], 1000, &ctx);
    assert!(cm.is_active());

    // Release both.
    let events = [
        InputEvent::VolButton(Edge::Deactivate),
        InputEvent::GainButton(Edge::Deactivate),
    ];
    cm.process_events(&events, 1100, &ctx);

    // Hold both again.
    let events = [
        InputEvent::VolButton(Edge::Activate),
        InputEvent::GainButton(Edge::Activate),
    ];
    cm.process_events(&events, 1200, &ctx);

    // Wait for hold duration.
    let result = cm.process_events(&[], 2200, &ctx);
    assert!(!cm.is_active());
    assert!(result
        .iter()
        .any(|e| matches!(e, ConfigDisplayEvent::Exited)));
}

#[test]
fn button_press_shows_feedback_when_active() {
    let mut cm = ConfigMode::new();
    let ctx = test_context();

    // Enter config mode.
    let events = [
        InputEvent::VolButton(Edge::Activate),
        InputEvent::GainButton(Edge::Activate),
    ];
    cm.process_events(&events, 0, &ctx);
    cm.process_events(&[], 1000, &ctx);

    // Press button C.
    let events = [InputEvent::ButtonC(Edge::Activate)];
    let result = cm.process_events(&events, 1100, &ctx);
    assert!(result
        .iter()
        .any(|e| matches!(e, ConfigDisplayEvent::ButtonPress { button: "C", .. })));
}

#[test]
fn encoder_turn_shows_feedback_when_active() {
    let mut cm = ConfigMode::new();
    let ctx = test_context();

    // Enter config mode.
    let events = [
        InputEvent::VolButton(Edge::Activate),
        InputEvent::GainButton(Edge::Activate),
    ];
    cm.process_events(&events, 0, &ctx);
    cm.process_events(&[], 1000, &ctx);

    // Turn encoder.
    let events = [InputEvent::Vol(Pulse::Clockwise)];
    let result = cm.process_events(&events, 1100, &ctx);
    assert!(result.iter().any(|e| matches!(
        e,
        ConfigDisplayEvent::EncoderTurn {
            encoder: "Vol",
            direction: _,
            ..
        }
    )));
}

#[test]
fn expression_pedal_shows_raw_adc_when_active() {
    let mut cm = ConfigMode::new();
    let ctx = test_context();

    // Enter config mode.
    let events = [
        InputEvent::VolButton(Edge::Activate),
        InputEvent::GainButton(Edge::Activate),
    ];
    cm.process_events(&events, 0, &ctx);
    cm.process_events(&[], 1000, &ctx);

    // Move pedal.
    let events = [InputEvent::ExpressionPedal1(3500)];
    let result = cm.process_events(&events, 1100, &ctx);
    assert!(result.iter().any(
        |e| matches!(e, ConfigDisplayEvent::ExpressionPedal { pedal: "Exp1", raw_adc: 3500, .. })
    ));
}

#[test]
fn expression_pedal2_shows_raw_adc() {
    let mut cm = ConfigMode::new();
    let ctx = test_context();

    // Enter config mode.
    let events = [
        InputEvent::VolButton(Edge::Activate),
        InputEvent::GainButton(Edge::Activate),
    ];
    cm.process_events(&events, 0, &ctx);
    cm.process_events(&[], 1000, &ctx);

    // Move pedal 2.
    let events = [InputEvent::ExpressionPedal2(1024)];
    let result = cm.process_events(&events, 1100, &ctx);
    assert!(result.iter().any(
        |e| matches!(e, ConfigDisplayEvent::ExpressionPedal { pedal: "Exp2", raw_adc: 1024, .. })
    ));
}

#[test]
fn no_events_when_inactive() {
    let mut cm = ConfigMode::new();
    let ctx = test_context();

    // Button press when NOT in config mode.
    let events = [InputEvent::ButtonA(Edge::Activate)];
    let result = cm.process_events(&events, 0, &ctx);
    assert!(result.is_empty());
}

#[test]
fn info_screen_on_entry() {
    let mut cm = ConfigMode::new();
    let ctx = test_context();

    let events = [
        InputEvent::VolButton(Edge::Activate),
        InputEvent::GainButton(Edge::Activate),
    ];
    cm.process_events(&events, 0, &ctx);
    let result = cm.process_events(&[], 1000, &ctx);

    assert!(result.iter().any(|e| matches!(e, ConfigDisplayEvent::Info(info) if info.preset_count == 5)));
}

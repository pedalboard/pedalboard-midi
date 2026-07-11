//! PE preset event handler: thin hardware adapter over protocol::controller.
//!
//! The firmware-specific responsibilities are:
//! - Map GPIO InputEvents to abstract Controller events
//! - Convert ActionStep to raw MIDI bytes for UART/USB output
//! - Render LED ring animations from button/encoder state
//!
//! All business logic lives in the Controller.

use crate::events::{Edge, InputEvent, Pulse};
use crate::ledring::{rgb8_to_rgb, Modifier, Renderer, RingAnimation};
#[cfg(target_arch = "arm")]
use crate::leds::LedEvent;
use midi_controller::config::{Color, Config, LedAnimation, LedRenderer, Preset};
use midi_controller::controller::{Controller, Event as CtrlEvent, Output};
use midi_controller::engine::ActionStep;
use midi_controller::long_press::Edge as LpEdge;
use midi_controller::state::PresetStateStore;
use smart_leds::RGB8;

const NUM_BUTTONS: usize = 6;

// Re-export types used by main.rs
pub use midi_controller::engine::{DisplayEvent, DisplaySide, SystemAction};

/// A step in an action sequence: raw MIDI bytes, a delay, or an LED change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiStep {
    Send([u8; 3], usize, midi_controller::routing::MidiPort),
    Delay(u16),
    SetLed {
        btn_idx: usize,
        color: Color,
        animation: LedAnimation,
    },
}

/// Result of processing events.
pub struct HandleResult {
    pub midi: heapless::Vec<MidiStep, 32>,
    pub display: heapless::Vec<DisplayEvent, 2>,
    pub routed: heapless::Vec<midi_controller::routing::MidiOut, 16>,
    pub reactive_led: Option<midi_controller::engine::ReactiveResult>,
    pub leds_changed: bool,
    pub preset_changed: bool,
    pub bpm: Option<u16>,
}

/// LED state for all 8 rings (A-F + Vol + Gain).
pub type LedAnimations = [RingAnimation; 8];

/// Stateful PE event handler. Wraps the protocol crate's Controller.
pub struct PeHandler {
    ctrl: Controller,
}

impl Default for PeHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl PeHandler {
    pub fn new() -> Self {
        Self {
            ctrl: Controller::new(),
        }
    }

    /// Create with a restored state store (from EEPROM).
    pub fn with_state(store: PresetStateStore) -> Self {
        Self {
            ctrl: Controller::with_state(store),
        }
    }

    /// Process input events against the config. Returns MIDI output + flags.
    pub fn handle_events(
        &mut self,
        config: &Config,
        events: &[InputEvent],
        now_ms: u32,
    ) -> HandleResult {
        let mut result = HandleResult {
            midi: heapless::Vec::new(),
            display: heapless::Vec::new(),
            routed: heapless::Vec::new(),
            reactive_led: None,
            leds_changed: false,
            preset_changed: false,
            bpm: None,
        };

        // Map hardware button events
        for i in 0..NUM_BUTTONS {
            if let Some(edge) = button_edge(events, i) {
                let r = self.ctrl.process(
                    CtrlEvent::ButtonEdge {
                        index: i as u8,
                        edge: edge_to_lp(edge),
                    },
                    now_ms,
                    config,
                );
                self.merge(&r, &mut result);
            }
        }

        // Tick for long-press detection
        if self.ctrl.button_held() {
            let r = self.ctrl.process(CtrlEvent::Tick, now_ms, config);
            self.merge(&r, &mut result);
        }

        // Map hardware encoder/analog events
        for event in events {
            match event {
                InputEvent::Vol(pulse) => {
                    let r = self.ctrl.process(
                        CtrlEvent::EncoderTurn {
                            index: 0,
                            clockwise: *pulse == Pulse::Clockwise,
                        },
                        now_ms,
                        config,
                    );
                    self.merge(&r, &mut result);
                }
                InputEvent::Gain(pulse) => {
                    let r = self.ctrl.process(
                        CtrlEvent::EncoderTurn {
                            index: 1,
                            clockwise: *pulse == Pulse::Clockwise,
                        },
                        now_ms,
                        config,
                    );
                    self.merge(&r, &mut result);
                }
                InputEvent::ExpressionPedal2(raw_adc) => {
                    let r = self.ctrl.process(
                        CtrlEvent::Analog {
                            index: 0,
                            raw: *raw_adc,
                        },
                        now_ms,
                        config,
                    );
                    self.merge(&r, &mut result);
                }
                InputEvent::ExpressionPedal1(raw_adc) => {
                    let r = self.ctrl.process(
                        CtrlEvent::Analog {
                            index: 1,
                            raw: *raw_adc,
                        },
                        now_ms,
                        config,
                    );
                    self.merge(&r, &mut result);
                }
                _ => {}
            }
        }

        result
    }

    /// Process incoming MIDI: routing, reactive LEDs, and triggers.
    pub fn process_incoming_midi(&mut self, config: &Config, raw: &[u8]) -> HandleResult {
        let mut data = [0u8; 8];
        let len = raw.len().min(8);
        data[..len].copy_from_slice(&raw[..len]);
        let mut r = self.ctrl.process(
            CtrlEvent::Midi {
                data,
                len: len as u8,
                source: midi_controller::routing::MidiPort::USB,
            },
            0,
            config,
        );
        let routed = core::mem::take(&mut r.midi_out);
        let reactive_led = r.reactive_led;
        let mut result = HandleResult {
            midi: heapless::Vec::new(),
            display: heapless::Vec::new(),
            routed,
            reactive_led,
            leds_changed: false,
            preset_changed: false,
            bpm: None,
        };
        self.merge(&r, &mut result);
        result
    }

    /// Serialize current state to EEPROM buffer.
    pub fn eeprom_state(&self) -> heapless::Vec<u8, 128> {
        let mut buf = [0u8; 128];
        let store = self.ctrl.snapshot_store();
        store.to_eeprom(&mut buf);
        heapless::Vec::from_slice(&buf).unwrap_or_default()
    }

    /// Returns true if any button is currently held.
    pub fn any_active(&self) -> bool {
        self.ctrl.button_held()
    }

    /// Returns the current button active state.
    pub fn button_active(&self) -> [bool; NUM_BUTTONS] {
        *self.ctrl.button_states()
    }

    /// Get the active preset index.
    pub fn active_preset(&self) -> u8 {
        self.ctrl.active_preset()
    }

    /// Set encoder value (for test/init).
    pub fn set_encoder_value(&mut self, index: usize, value: u8) {
        self.ctrl.set_encoder_value(index, value);
    }

    /// Switch to a preset (for boot initialization).
    pub fn switch_to(&mut self, preset_idx: u8, config: &Config) -> HandleResult {
        let r = self.ctrl.select_preset(preset_idx, config);
        let mut result = HandleResult {
            midi: heapless::Vec::new(),
            display: heapless::Vec::new(),
            routed: heapless::Vec::new(),
            reactive_led: None,
            leds_changed: false,
            preset_changed: false,
            bpm: None,
        };
        self.merge(&r, &mut result);
        result
    }

    /// Compute LED animations for all 8 rings.
    pub fn led_state(&self, preset: &Preset) -> LedAnimations {
        let mut anims = [RingAnimation::off(); 8];
        let button_active = self.ctrl.button_states();
        let encoder_values = self.ctrl.encoder_values();

        for (i, anim) in anims.iter_mut().enumerate().take(NUM_BUTTONS) {
            if let Some(btn) = preset.buttons.get(i) {
                if btn.listen_cc.is_some() {
                    continue;
                }
                let on_color = color_to_rgb(&btn.color.on);
                if on_color == RGB8::default() {
                    *anim = RingAnimation::off();
                } else if button_active[i] {
                    let modifier = anim_to_modifier(btn.color.animation);
                    let rgb = rgb8_to_rgb(on_color);
                    let renderer =
                        renderer_from_config(rgb, btn.color.renderer, btn.color.renderer_param);
                    *anim = RingAnimation { renderer, modifier };
                } else if btn.color.off == Color::Off {
                    let rgb = rgb8_to_rgb(on_color);
                    let renderer =
                        renderer_from_config(rgb, btn.color.renderer, btn.color.renderer_param);
                    *anim = RingAnimation {
                        renderer,
                        modifier: Modifier::Glow,
                    };
                } else {
                    let off_color = color_to_rgb(&btn.color.off);
                    *anim = RingAnimation::solid(rgb8_to_rgb(off_color));
                };
            }
        }

        let fill_vol = ((encoder_values[0] as u16 * 12) / 127).min(12) as u8;
        anims[6] = RingAnimation {
            renderer: Renderer::Heatmap(fill_vol),
            modifier: Modifier::Solid,
        };
        let fill_gain = ((encoder_values[1] as u16 * 12) / 127).min(12) as u8;
        anims[7] = RingAnimation {
            renderer: Renderer::Heatmap(fill_gain),
            modifier: Modifier::Solid,
        };

        anims
    }

    // --- Private ---

    fn merge(&self, ctrl_result: &Output, result: &mut HandleResult) {
        for step in &ctrl_result.midi {
            match step {
                ActionStep::Send(msg) => {
                    result
                        .midi
                        .push(MidiStep::Send(msg.data, msg.len, msg.dest))
                        .ok();
                }
                ActionStep::Delay(ms) => {
                    result.midi.push(MidiStep::Delay(*ms)).ok();
                }
                ActionStep::SetLed { color, animation } => {
                    result
                        .midi
                        .push(MidiStep::SetLed {
                            btn_idx: 0,
                            color: *color,
                            animation: *animation,
                        })
                        .ok();
                }
            }
        }
        for d in &ctrl_result.display {
            result.display.push(d.clone()).ok();
        }
        if ctrl_result.leds_changed {
            result.leds_changed = true;
        }
        if ctrl_result.preset_changed {
            result.preset_changed = true;
        }
        if let Some(bpm) = ctrl_result.bpm {
            result.bpm = Some(bpm);
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

pub fn color_to_rgb(c: &Color) -> RGB8 {
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

fn anim_to_modifier(anim: LedAnimation) -> Modifier {
    match anim {
        LedAnimation::Solid => Modifier::Solid,
        LedAnimation::Blink => Modifier::Blink,
        LedAnimation::Pulse => Modifier::Pulse,
        LedAnimation::Rotate => Modifier::Rotate,
        LedAnimation::ColorCycle => Modifier::ColorCycle,
    }
}

fn renderer_from_config(rgb: crate::ledring::Rgb, renderer: LedRenderer, param: u8) -> Renderer {
    match renderer {
        LedRenderer::Solid => Renderer::Solid(rgb),
        LedRenderer::Fill => Renderer::Fill(rgb, param.max(1)),
        LedRenderer::Single => Renderer::Single(rgb, param),
        LedRenderer::Dots => Renderer::Dots(rgb, param.max(1)),
        LedRenderer::Heatmap => Renderer::Heatmap(param),
    }
}

/// Build the "on" ring animation for a button from its preset config.
pub fn button_ring_animation(preset: &Preset, btn_idx: usize) -> RingAnimation {
    let Some(btn) = preset.buttons.get(btn_idx) else {
        return RingAnimation::off();
    };
    let on_color = color_to_rgb(&btn.color.on);
    if on_color == RGB8::default() {
        return RingAnimation::off();
    }
    let modifier = anim_to_modifier(btn.color.animation);
    let rgb = rgb8_to_rgb(on_color);
    let renderer = renderer_from_config(rgb, btn.color.renderer, btn.color.renderer_param);
    RingAnimation { renderer, modifier }
}

fn edge_to_lp(edge: Edge) -> LpEdge {
    match edge {
        Edge::Activate => LpEdge::Activate,
        Edge::Deactivate => LpEdge::Deactivate,
    }
}

/// Process an incoming CC message against a preset's reactive LED bindings.
/// Returns a LedEvent if the CC triggers a reactive ring update.
#[cfg(target_arch = "arm")]
pub fn reactive_led_event(preset: &Preset, channel: u8, cc: u8, value: u8) -> Option<LedEvent> {
    use midi_controller::engine::{process_incoming_cc, ReactiveResult};

    let result = process_incoming_cc(preset, channel, cc, value)?;
    let evt = match result {
        ReactiveResult::Heatmap(idx, fill) => LedEvent::SetReactiveRing(idx, fill),
        ReactiveResult::Trigger(idx, active) => {
            let anim = if active {
                Some(button_ring_animation(preset, idx))
            } else {
                None
            };
            LedEvent::SetReactiveTrigger(idx, anim)
        }
    };
    Some(evt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Edge, InputEvent, Pulse};
    use midi_controller::config::*;

    /// Helper: build a config with one preset containing a single CC button + encoder + analog.
    fn cc_button_config() -> Config {
        let mut buttons: heapless::Vec<ButtonConfig, MAX_BUTTONS> = heapless::Vec::new();
        buttons
            .push(ButtonConfig {
                label: Label::new(),
                color: LedConfig {
                    on: Color::Blue,
                    off: Color::Off,
                    animation: LedAnimation::Solid,
                    renderer: LedRenderer::Solid,
                    renderer_param: 0,
                },
                mode: ButtonMode::Momentary,
                on_press: {
                    let mut v = heapless::Vec::new();
                    v.push(Action::cc(80, 127, 1).unwrap()).ok();
                    v
                },
                on_release: heapless::Vec::new(),
                on_long_press: heapless::Vec::new(),
                cycle_values: heapless::Vec::new(),
                listen_cc: None,
            })
            .ok();

        let mut encoders: heapless::Vec<EncoderConfig, MAX_ENCODERS> = heapless::Vec::new();
        encoders
            .push(EncoderConfig {
                label: Label::try_from("Vol").unwrap(),
                action: EncoderAction::Cc {
                    cc: 7,
                    channel: 1,
                    min: 0,
                    max: 127,
                },
                ..Default::default()
            })
            .ok();

        let mut analog: heapless::Vec<AnalogConfig, MAX_ANALOG> = heapless::Vec::new();
        // Index 0 = ExpressionPedal2 (mapped in handle_events), Index 1 = ExpressionPedal1
        analog
            .push(AnalogConfig {
                label: Label::new(),
                cc: 11,
                channel: 1,
                min: 0,
                max: 127,
            })
            .ok();
        analog
            .push(AnalogConfig {
                label: Label::new(),
                cc: 12,
                channel: 1,
                min: 0,
                max: 127,
            })
            .ok();

        let preset = Preset {
            name: Label::try_from("Test").unwrap(),
            buttons,
            encoders,
            analog,
            defaults: Default::default(),
            on_enter: heapless::Vec::new(),
            on_exit: heapless::Vec::new(),
            triggers: heapless::Vec::new(),
        };

        let mut presets: heapless::Vec<Preset, MAX_PRESETS> = heapless::Vec::new();
        presets.push(preset).ok();
        Config {
            global: GlobalConfig::default(),
            presets,
        }
    }

    /// Helper: config with on_enter actions on second preset.
    fn on_enter_config() -> Config {
        let empty_preset = Preset {
            name: Label::try_from("First").unwrap(),
            buttons: heapless::Vec::new(),
            encoders: heapless::Vec::new(),
            analog: heapless::Vec::new(),
            defaults: Default::default(),
            on_enter: heapless::Vec::new(),
            on_exit: heapless::Vec::new(),
            triggers: heapless::Vec::new(),
        };

        let second_preset = Preset {
            name: Label::try_from("Second").unwrap(),
            buttons: heapless::Vec::new(),
            encoders: heapless::Vec::new(),
            analog: heapless::Vec::new(),
            defaults: Default::default(),
            on_enter: {
                let mut v = heapless::Vec::new();
                v.push(Action::cc(99, 127, 2).unwrap()).ok();
                v
            },
            on_exit: heapless::Vec::new(),
            triggers: heapless::Vec::new(),
        };

        let mut presets: heapless::Vec<Preset, MAX_PRESETS> = heapless::Vec::new();
        presets.push(empty_preset).ok();
        presets.push(second_preset).ok();
        Config {
            global: GlobalConfig::default(),
            presets,
        }
    }

    // --- Test 1 ---
    #[test]
    fn button_press_generates_cc() {
        let config = cc_button_config();
        let mut h = PeHandler::new();
        let r = h.handle_events(&config, &[InputEvent::ButtonA(Edge::Activate)], 0);
        assert!(!r.midi.is_empty(), "expected MIDI output on button press");
        let step = &r.midi[0];
        assert!(
            matches!(step, MidiStep::Send(d, 3, _) if d[0] == 0xB0 && d[1] == 80 && d[2] == 127),
            "expected CC 80 value 127 on ch1, got {:?}",
            step
        );
    }

    // --- Test 2 ---
    #[test]
    fn button_release_no_output() {
        let config = cc_button_config();
        let mut h = PeHandler::new();
        // Press first to set state
        h.handle_events(&config, &[InputEvent::ButtonA(Edge::Activate)], 0);
        // Release — momentary button with no on_release actions
        let r = h.handle_events(&config, &[InputEvent::ButtonA(Edge::Deactivate)], 10);
        let midi_sends: heapless::Vec<&MidiStep, 32> = r
            .midi
            .iter()
            .filter(|s| matches!(s, MidiStep::Send(_, _, _)))
            .collect();
        assert!(
            midi_sends.is_empty(),
            "expected no MIDI on release of momentary button with no on_release, got {:?}",
            midi_sends
        );
    }

    // --- Test 3 ---
    #[test]
    fn encoder_clockwise_increments_cc() {
        let config = cc_button_config();
        let mut h = PeHandler::new();
        h.set_encoder_value(0, 64);
        let r = h.handle_events(&config, &[InputEvent::Vol(Pulse::Clockwise)], 0);
        assert!(!r.midi.is_empty(), "expected MIDI output from encoder CW");
        let step = &r.midi[0];
        // Value should be 65 (64 + 1)
        assert!(
            matches!(step, MidiStep::Send(d, 3, _) if d[0] == 0xB0 && d[1] == 7 && d[2] == 65),
            "expected CC 7 value 65, got {:?}",
            step
        );
    }

    // --- Test 4 ---
    #[test]
    fn encoder_counterclockwise_decrements_cc() {
        let config = cc_button_config();
        let mut h = PeHandler::new();
        h.set_encoder_value(0, 64);
        let r = h.handle_events(&config, &[InputEvent::Vol(Pulse::CounterClockwise)], 0);
        assert!(!r.midi.is_empty(), "expected MIDI output from encoder CCW");
        let step = &r.midi[0];
        // Value should be 63 (64 - 1)
        assert!(
            matches!(step, MidiStep::Send(d, 3, _) if d[0] == 0xB0 && d[1] == 7 && d[2] == 63),
            "expected CC 7 value 63, got {:?}",
            step
        );
    }

    // --- Test 5 ---
    #[test]
    fn expression_pedal_generates_cc() {
        let config = cc_button_config();
        let mut h = PeHandler::new();
        // ExpressionPedal1 maps to analog index 1 (cc 12, channel 1)
        // ADC mid-point: 2048 out of default range 0..3750
        let r = h.handle_events(&config, &[InputEvent::ExpressionPedal1(2048)], 0);
        // Should produce a CC message on channel 1, cc 12
        let cc_msgs: heapless::Vec<&MidiStep, 32> = r
            .midi
            .iter()
            .filter(|s| matches!(s, MidiStep::Send(d, 3, _) if d[0] == 0xB0 && d[1] == 12))
            .collect();
        assert!(
            !cc_msgs.is_empty(),
            "expected CC 12 output from expression pedal, got {:?}",
            r.midi
        );
        // Value should be proportional — roughly 2048/3750 * 127 ≈ 69
        if let MidiStep::Send(d, _, _) = cc_msgs[0] {
            assert!(
                d[2] > 50 && d[2] < 90,
                "expected proportional CC value around 69, got {}",
                d[2]
            );
        }
    }

    // --- Test 6 ---
    #[test]
    fn preset_switch_fires_on_enter() {
        let config = on_enter_config();
        let mut h = PeHandler::new();
        // Switch from preset 0 to preset 1 (which has on_enter CC 99)
        let r = h.switch_to(1, &config);
        assert!(r.preset_changed, "expected preset_changed flag");
        let cc_msgs: heapless::Vec<&MidiStep, 32> = r
            .midi
            .iter()
            .filter(
                |s| matches!(s, MidiStep::Send(d, 3, _) if d[0] == 0xB1 && d[1] == 99 && d[2] == 127),
            )
            .collect();
        assert!(
            !cc_msgs.is_empty(),
            "expected on_enter CC 99 on ch2, got {:?}",
            r.midi
        );
    }

    // --- Test 7 ---
    #[test]
    fn led_state_active_button_shows_color() {
        let config = cc_button_config();
        let mut h = PeHandler::new();
        // Press button A to make it active
        h.handle_events(&config, &[InputEvent::ButtonA(Edge::Activate)], 0);
        let preset = &config.presets[0];
        let leds = h.led_state(preset);
        // Button A (index 0) should show on-color (Blue) with Solid modifier
        let expected_renderer = Renderer::Solid(crate::ledring::Rgb::new(0, 0, 255));
        assert_eq!(
            leds[0].renderer, expected_renderer,
            "expected blue solid renderer for active button"
        );
        assert_eq!(
            leds[0].modifier,
            Modifier::Solid,
            "expected Solid modifier for active button"
        );
    }

    // --- Test 8 ---
    #[test]
    fn led_state_inactive_button_shows_glow() {
        let config = cc_button_config();
        let h = PeHandler::new();
        let preset = &config.presets[0];
        let leds = h.led_state(preset);
        // Button A (index 0) is not pressed, off == Color::Off → should show Glow modifier
        let expected_renderer = Renderer::Solid(crate::ledring::Rgb::new(0, 0, 255));
        assert_eq!(
            leds[0].renderer, expected_renderer,
            "expected blue renderer for inactive button with off=Off"
        );
        assert_eq!(
            leds[0].modifier,
            Modifier::Glow,
            "expected Glow modifier for inactive button"
        );
    }

    // --- Test 9 ---
    #[test]
    fn led_state_encoder_heatmap() {
        let config = cc_button_config();
        let mut h = PeHandler::new();
        h.set_encoder_value(0, 64);
        let preset = &config.presets[0];
        let leds = h.led_state(preset);
        // Encoder 0 (Vol) → anims[6], fill = (64 * 12) / 127 = 6
        let expected_fill = ((64u16 * 12) / 127).min(12) as u8;
        assert_eq!(
            leds[6].renderer,
            Renderer::Heatmap(expected_fill),
            "expected heatmap fill={} for encoder value 64",
            expected_fill
        );
        assert_eq!(leds[6].modifier, Modifier::Solid);
    }

    // --- Test 10 ---
    #[test]
    fn process_incoming_midi_routes_to_output() {
        // Enable USB → DIN thru routing
        let mut config = cc_button_config();
        config.global.usb_to_din_thru = true;
        let mut h = PeHandler::new();
        // Send a CC message as if received from USB
        let raw = [0xB0, 44, 100]; // CC 44, value 100, channel 1
        let r = h.process_incoming_midi(&config, &raw);
        // Should be routed to DIN output
        assert!(
            !r.routed.is_empty(),
            "expected routed output for thru, got empty"
        );
        let routed = &r.routed[0];
        assert_eq!(
            &routed.data[..routed.len as usize],
            &[0xB0, 44, 100],
            "expected same CC bytes in routed output"
        );
    }
}

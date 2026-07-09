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
    Send([u8; 3], usize),
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

    /// Process incoming MIDI against preset triggers.
    pub fn process_incoming_midi(&mut self, config: &Config, raw: &[u8]) -> HandleResult {
        let mut data = [0u8; 8];
        let len = raw.len().min(8);
        data[..len].copy_from_slice(&raw[..len]);
        let r = self.ctrl.process(
            CtrlEvent::Midi {
                data,
                len: len as u8,
                source: midi_controller::routing::MidiPort::USB,
            },
            0,
            config,
        );
        let mut result = HandleResult {
            midi: heapless::Vec::new(),
            display: heapless::Vec::new(),
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
                    result.midi.push(MidiStep::Send(msg.data, msg.len)).ok();
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

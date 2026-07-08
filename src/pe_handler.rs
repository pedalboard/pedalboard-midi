//! PE preset event handler: thin hardware adapter over protocol::controller.
//!
//! Responsibilities (HMI/hardware only):
//! - Input event → abstract InputEvent mapping
//! - Format conversion (ActionStep → MidiStep with raw bytes)
//! - LED ring rendering (hardware-specific RGB animations)
//! - On_enter/on_exit actions (preset switch lifecycle)
//! - MIDI trigger processing
//!
//! All timing and button/encoder logic is delegated to the Controller.

use crate::events::{Edge, InputEvent, Pulse};
use crate::ledring::{rgb8_to_rgb, Modifier, Renderer, RingAnimation};
use pedalboard_protocol::config::{Color, LedAnimation, LedRenderer, Preset};
use pedalboard_protocol::controller::{Controller, ControllerResult, InputEvent as CtrlEvent};
use pedalboard_protocol::engine::ActionStep;
use pedalboard_protocol::long_press::Edge as LpEdge;
use pedalboard_protocol::state::PresetStateStore;
use smart_leds::RGB8;

const NUM_BUTTONS: usize = 6;

/// ADC calibration values per expression pedal (loaded from GlobalConfig).
pub struct AdcCalibration {
    pub exp1_min: u16,
    pub exp1_max: u16,
    pub exp2_min: u16,
    pub exp2_max: u16,
}

impl Default for AdcCalibration {
    fn default() -> Self {
        Self {
            exp1_min: 0,
            exp1_max: 3750,
            exp2_min: 0,
            exp2_max: 3750,
        }
    }
}

// Re-export types used by main.rs
pub use pedalboard_protocol::engine::{DisplayEvent, DisplaySide, SystemAction};

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

/// Result of processing events: MIDI steps + system actions + display + LED dirty flag.
pub struct HandleResult {
    pub midi: heapless::Vec<MidiStep, 8>,
    pub system: heapless::Vec<SystemAction, 2>,
    pub display: heapless::Vec<DisplayEvent, 2>,
    pub led_dirty: bool,
}

/// LED state for all 8 rings (A-F + Vol + Gain).
pub type LedAnimations = [RingAnimation; 8];

/// Stateful PE event handler. Wraps the protocol crate's Controller and adds
/// hardware-specific concerns (LED rendering, MIDI format, on_enter/on_exit).
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

    /// Switch to a new preset: saves current state, loads new state,
    /// fires on_exit/on_enter actions, and returns recall MIDI.
    pub fn switch_preset(
        &mut self,
        new_preset: u8,
        old_preset: &Preset,
        new_preset_cfg: &Preset,
    ) -> heapless::Vec<MidiStep, 32> {
        let mut result: heapless::Vec<MidiStep, 32> = heapless::Vec::new();

        // 1. Fire on_exit for old preset
        for action in &old_preset.on_exit {
            match action {
                pedalboard_protocol::config::Action::Delay(ms) => {
                    result.push(MidiStep::Delay(*ms)).ok();
                }
                _ => {
                    if let Some(msg) = pedalboard_protocol::action::action_to_midi(action) {
                        result.push(MidiStep::Send(msg.data, msg.len)).ok();
                    }
                }
            }
        }

        // 2. Delegate state save/load/recall to Controller
        let recall = self.ctrl.switch_preset(new_preset, new_preset_cfg);

        // 3. Fire on_enter for new preset
        for action in &new_preset_cfg.on_enter {
            match action {
                pedalboard_protocol::config::Action::Delay(ms) => {
                    result.push(MidiStep::Delay(*ms)).ok();
                }
                _ => {
                    if let Some(msg) = pedalboard_protocol::action::action_to_midi(action) {
                        result.push(MidiStep::Send(msg.data, msg.len)).ok();
                    }
                }
            }
        }

        // 4. Recall MIDI (active toggles + encoder values)
        for step in &recall {
            if let ActionStep::Send(msg) = step {
                result.push(MidiStep::Send(msg.data, msg.len)).ok();
            }
        }

        result
    }

    /// Serialize current state to a 128-byte EEPROM buffer for persistence.
    pub fn eeprom_state(&self) -> heapless::Vec<u8, 128> {
        self.ctrl.eeprom_state()
    }

    /// Returns true if any button is currently held (long-press counting).
    pub fn any_active(&self) -> bool {
        self.ctrl.any_active()
    }

    /// Returns the current button active state (toggle ON / momentary held).
    pub fn button_active(&self) -> [bool; NUM_BUTTONS] {
        *self.ctrl.button_active()
    }

    /// Get the current encoder values.
    pub fn encoder_values(&self) -> [u8; 2] {
        self.ctrl.encoder_values()
    }

    /// Set encoder value (for test setup or initial state from config).
    pub fn set_encoder_value(&mut self, index: usize, value: u8) {
        let mut state = self.ctrl.state().clone();
        if index < 2 {
            state.encoder_values[index] = value;
        }
        self.ctrl.save_working(&state);
    }

    /// Process input events against a PE preset. Returns MIDI messages and system actions.
    pub fn handle_events(
        &mut self,
        preset: &Preset,
        events: &[InputEvent],
        cal: &AdcCalibration,
        now_ms: u32,
    ) -> HandleResult {
        let mut result = HandleResult {
            midi: heapless::Vec::new(),
            system: heapless::Vec::new(),
            display: heapless::Vec::new(),
            led_dirty: false,
        };

        // Map hardware events to abstract Controller events and process
        for i in 0..NUM_BUTTONS {
            if let Some(edge) = button_edge(events, i) {
                let ctrl_result = self.ctrl.process(
                    CtrlEvent::ButtonEdge {
                        index: i as u8,
                        edge: edge_to_lp(edge),
                    },
                    now_ms,
                    preset,
                );
                self.merge_ctrl_result(&ctrl_result, i, &mut result);
            }
        }

        // Tick for long-press detection on buttons with no edge this cycle
        if self.ctrl.any_active() {
            let tick_result = self.ctrl.tick(now_ms, preset);
            self.merge_ctrl_result(&tick_result, 0, &mut result);
        }

        // Encoders
        for event in events {
            match event {
                InputEvent::Vol(pulse) => {
                    let ctrl_result = self.ctrl.process(
                        CtrlEvent::EncoderTurn {
                            index: 0,
                            clockwise: *pulse == Pulse::Clockwise,
                        },
                        now_ms,
                        preset,
                    );
                    self.merge_ctrl_result(&ctrl_result, 0, &mut result);
                }
                InputEvent::Gain(pulse) => {
                    let ctrl_result = self.ctrl.process(
                        CtrlEvent::EncoderTurn {
                            index: 1,
                            clockwise: *pulse == Pulse::Clockwise,
                        },
                        now_ms,
                        preset,
                    );
                    self.merge_ctrl_result(&ctrl_result, 0, &mut result);
                }
                InputEvent::ExpressionPedal2(raw_adc) => {
                    let ctrl_result = self.ctrl.process(
                        CtrlEvent::Analog {
                            index: 0,
                            raw: *raw_adc,
                            min: cal.exp2_min,
                            max: cal.exp2_max,
                        },
                        now_ms,
                        preset,
                    );
                    self.merge_ctrl_result(&ctrl_result, 0, &mut result);
                }
                InputEvent::ExpressionPedal1(raw_adc) => {
                    let ctrl_result = self.ctrl.process(
                        CtrlEvent::Analog {
                            index: 1,
                            raw: *raw_adc,
                            min: cal.exp1_min,
                            max: cal.exp1_max,
                        },
                        now_ms,
                        preset,
                    );
                    self.merge_ctrl_result(&ctrl_result, 0, &mut result);
                }
                _ => {}
            }
        }

        result
    }

    /// Process incoming MIDI against preset triggers. Returns MIDI steps + system actions.
    pub fn process_incoming_midi(&mut self, preset: &Preset, raw: &[u8]) -> HandleResult {
        use pedalboard_protocol::engine::process_triggers;
        use pedalboard_protocol::state::PresetState;

        let mut result = HandleResult {
            midi: heapless::Vec::new(),
            system: heapless::Vec::new(),
            display: heapless::Vec::new(),
            led_dirty: false,
        };

        if raw.len() >= 2 {
            // Build a working state from the controller's current state
            let button_active = *self.ctrl.button_active();
            let encoder_values = self.ctrl.encoder_values();
            let mut state = PresetState {
                button_active,
                encoder_values,
                cycle_index: self.ctrl.state().cycle_index,
            };
            let data2 = if raw.len() >= 3 { raw[2] } else { 0 };
            let trigger_result = process_triggers(&mut state, preset, raw[0], raw[1], data2);

            // Apply trigger state changes back
            self.ctrl.save_working(&state);

            // Convert to MidiSteps
            for step in &trigger_result.midi {
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
            for s in &trigger_result.system {
                result.system.push(*s).ok();
            }
            result.led_dirty = trigger_result.led_dirty;
        }

        result
    }

    /// Compute LED animations for all 8 rings based on current state + preset config.
    pub fn led_state(&self, preset: &Preset) -> LedAnimations {
        let mut anims = [RingAnimation::off(); 8];
        let button_active = self.ctrl.button_active();
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
                    let modifier = match btn.color.animation {
                        LedAnimation::Solid => Modifier::Solid,
                        LedAnimation::Blink => Modifier::Blink,
                        LedAnimation::Pulse => Modifier::Pulse,
                        LedAnimation::Rotate => Modifier::Rotate,
                        LedAnimation::ColorCycle => Modifier::ColorCycle,
                    };
                    let rgb = rgb8_to_rgb(on_color);
                    let renderer = match btn.color.renderer {
                        LedRenderer::Solid => Renderer::Solid(rgb),
                        LedRenderer::Fill => Renderer::Fill(rgb, btn.color.renderer_param.max(1)),
                        LedRenderer::Single => Renderer::Single(rgb, btn.color.renderer_param),
                        LedRenderer::Dots => Renderer::Dots(rgb, btn.color.renderer_param.max(1)),
                    };
                    *anim = RingAnimation { renderer, modifier };
                } else if btn.color.off == Color::Off {
                    let rgb = rgb8_to_rgb(on_color);
                    let renderer = match btn.color.renderer {
                        LedRenderer::Solid => Renderer::Solid(rgb),
                        LedRenderer::Fill => Renderer::Fill(rgb, btn.color.renderer_param.max(1)),
                        LedRenderer::Single => Renderer::Single(rgb, btn.color.renderer_param),
                        LedRenderer::Dots => Renderer::Dots(rgb, btn.color.renderer_param.max(1)),
                    };
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

    // --- Private helpers ---

    fn merge_ctrl_result(
        &self,
        ctrl_result: &ControllerResult,
        btn_idx: usize,
        result: &mut HandleResult,
    ) {
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
                            btn_idx,
                            color: *color,
                            animation: *animation,
                        })
                        .ok();
                }
            }
        }
        for s in &ctrl_result.system {
            result.system.push(*s).ok();
        }
        for d in &ctrl_result.display {
            result.display.push(d.clone()).ok();
        }
        if ctrl_result.led_dirty {
            result.led_dirty = true;
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

/// Build the "on" ring animation for a button from its preset config.
pub fn button_ring_animation(preset: &Preset, btn_idx: usize) -> RingAnimation {
    let Some(btn) = preset.buttons.get(btn_idx) else {
        return RingAnimation::off();
    };
    let on_color = color_to_rgb(&btn.color.on);
    if on_color == RGB8::default() {
        return RingAnimation::off();
    }
    let modifier = match btn.color.animation {
        LedAnimation::Solid => Modifier::Solid,
        LedAnimation::Blink => Modifier::Blink,
        LedAnimation::Pulse => Modifier::Pulse,
        LedAnimation::Rotate => Modifier::Rotate,
        LedAnimation::ColorCycle => Modifier::ColorCycle,
    };
    let rgb = rgb8_to_rgb(on_color);
    let renderer = match btn.color.renderer {
        LedRenderer::Solid => Renderer::Solid(rgb),
        LedRenderer::Fill => Renderer::Fill(rgb, btn.color.renderer_param.max(1)),
        LedRenderer::Single => Renderer::Single(rgb, btn.color.renderer_param),
        LedRenderer::Dots => Renderer::Dots(rgb, btn.color.renderer_param.max(1)),
    };
    RingAnimation { renderer, modifier }
}

/// Convert firmware Edge to protocol long_press Edge.
fn edge_to_lp(edge: Edge) -> LpEdge {
    match edge {
        Edge::Activate => LpEdge::Activate,
        Edge::Deactivate => LpEdge::Deactivate,
    }
}

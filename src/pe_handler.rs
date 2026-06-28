//! PE preset event handler: processes input events against a PE preset config.

use crate::action::{action_to_midi, analog_cc, encoder_cc, EncoderDirection, MidiMessage};
use crate::events::{Edge, InputEvent, Pulse};
use crate::ledring::Animation;
use crate::long_press::{Gesture, LongPressDetector};
use pedalboard_protocol::config::{Action, ButtonMode, Color, Preset};
use smart_leds::RGB8;

const NUM_BUTTONS: usize = 6;
/// ADC upper trim — hardware doesn't reach full 4095. Matches UpperADCOffset(14) from board design.
const ADC_MAX_TRIMMED: u16 = 3750;

/// System-level actions that transcend MIDI output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAction {
    PresetNext,
    PresetPrev,
}

/// Which display to show an overlay on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplaySide {
    L,
    R,
}

/// Display events emitted directly from actions (no MIDI round-trip).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayEvent {
    EncoderOverlay {
        side: DisplaySide,
        label: pedalboard_protocol::config::Label,
        value: u8,
    },
    AnalogOverlay {
        side: DisplaySide,
        label: pedalboard_protocol::config::Label,
        value: u8,
    },
}

/// Result of processing events: MIDI messages + system actions + display + LED dirty flag.
pub struct HandleResult {
    pub midi: heapless::Vec<([u8; 3], usize), 8>,
    pub system: heapless::Vec<SystemAction, 2>,
    pub display: heapless::Vec<DisplayEvent, 2>,
    pub led_dirty: bool,
}

/// LED state for all 8 rings (A-F + Vol + Gain).
pub type LedAnimations = [Animation; 8];

/// Stateful PE event handler. Tracks encoder values, button state, and long-press.
pub struct PeHandler {
    pub encoder_values: [u8; 2],
    button_active: [bool; NUM_BUTTONS],
    cycle_index: [u8; NUM_BUTTONS],
    last_encoder_tick: [u16; 2],
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
            button_active: [false; NUM_BUTTONS],
            cycle_index: [0; NUM_BUTTONS],
            last_encoder_tick: [u16::MAX; 2],
            long_press: core::array::from_fn(|_| LongPressDetector::new()),
        }
    }

    /// Call every 1ms unconditionally to keep encoder acceleration timing accurate.
    pub fn tick(&mut self) {
        self.last_encoder_tick[0] = self.last_encoder_tick[0].saturating_add(1);
        self.last_encoder_tick[1] = self.last_encoder_tick[1].saturating_add(1);
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
        let mut display = heapless::Vec::new();
        let mut led_dirty = false;

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

            let mode = preset
                .buttons
                .get(i)
                .map(|b| &b.mode)
                .unwrap_or(&ButtonMode::Momentary);

            if has_long_press {
                // Track physical press for LED (momentary visual feedback)
                if matches!(mode, &ButtonMode::Momentary) {
                    match edge {
                        Some(Edge::Activate) => {
                            self.button_active[i] = true;
                            led_dirty = true;
                        }
                        Some(Edge::Deactivate) => {
                            self.button_active[i] = false;
                            led_dirty = true;
                        }
                        None => {}
                    }
                }
                match self.long_press[i].update(edge) {
                    Some(Gesture::ShortPress) => {
                        if let Some(btn) = preset.buttons.get(i) {
                            if mode == &ButtonMode::Toggle {
                                self.button_active[i] = !self.button_active[i];
                                led_dirty = true;
                            } else if let ButtonMode::RadioGroup(group) = mode {
                                for j in 0..NUM_BUTTONS {
                                    if j != i {
                                        if let Some(other) = preset.buttons.get(j) {
                                            if other.mode == ButtonMode::RadioGroup(*group) {
                                                self.button_active[j] = false;
                                            }
                                        }
                                    }
                                }
                                self.button_active[i] = true;
                                led_dirty = true;
                            }
                            execute_actions(
                                &btn.on_press,
                                &btn.cycle_values,
                                &mut midi,
                                &mut system,
                                &mut self.cycle_index[i],
                            );
                            execute_actions(
                                &btn.on_release,
                                &btn.cycle_values,
                                &mut midi,
                                &mut system,
                                &mut self.cycle_index[i],
                            );
                        }
                    }
                    Some(Gesture::LongPress) => {
                        if let Some(btn) = preset.buttons.get(i) {
                            execute_actions(
                                &btn.on_long_press,
                                &btn.cycle_values,
                                &mut midi,
                                &mut system,
                                &mut self.cycle_index[i],
                            );
                        }
                    }
                    None => {}
                }
            } else {
                match edge {
                    Some(Edge::Activate) => {
                        if let Some(btn) = preset.buttons.get(i) {
                            match mode {
                                ButtonMode::Toggle => {
                                    self.button_active[i] = !self.button_active[i];
                                    led_dirty = true;
                                }
                                ButtonMode::Momentary => {
                                    self.button_active[i] = true;
                                    led_dirty = true;
                                }
                                ButtonMode::RadioGroup(group) => {
                                    // Deactivate all others in same group
                                    for j in 0..NUM_BUTTONS {
                                        if j != i {
                                            if let Some(other) = preset.buttons.get(j) {
                                                if other.mode == ButtonMode::RadioGroup(*group) {
                                                    self.button_active[j] = false;
                                                }
                                            }
                                        }
                                    }
                                    self.button_active[i] = true;
                                    led_dirty = true;
                                }
                            }
                            execute_actions(
                                &btn.on_press,
                                &btn.cycle_values,
                                &mut midi,
                                &mut system,
                                &mut self.cycle_index[i],
                            );
                        }
                    }
                    Some(Edge::Deactivate) => {
                        if let Some(btn) = preset.buttons.get(i) {
                            if matches!(mode, ButtonMode::Momentary) {
                                self.button_active[i] = false;
                                led_dirty = true;
                            }
                            execute_actions(
                                &btn.on_release,
                                &btn.cycle_values,
                                &mut midi,
                                &mut system,
                                &mut self.cycle_index[i],
                            );
                        }
                    }
                    None => {}
                }
            }
        }

        // Encoder/analog events also dirty LEDs (heatmap updates)
        // Encoder/analog events
        for event in events {
            match event {
                InputEvent::Vol(pulse) => {
                    let dir = pulse_to_dir(*pulse);
                    let steps = accel_steps(self.last_encoder_tick[0]);
                    self.last_encoder_tick[0] = 0;
                    for _ in 0..steps {
                        encoder_cc(preset, 0, dir, &mut self.encoder_values[0]);
                    }
                    // Send only the final value
                    if let Some(enc) = preset.encoders.first() {
                        if let pedalboard_protocol::config::EncoderAction::Cc {
                            cc, channel, ..
                        } = &enc.action
                        {
                            push_midi(
                                &mut midi,
                                &MidiMessage {
                                    data: [0xB0 | (channel - 1), *cc as u8, self.encoder_values[0]],
                                    len: 3,
                                },
                            );
                        }
                        display
                            .push(DisplayEvent::EncoderOverlay {
                                side: DisplaySide::L,
                                label: enc.label.clone(),
                                value: self.encoder_values[0],
                            })
                            .ok();
                    }
                    led_dirty = true;
                }
                InputEvent::Gain(pulse) => {
                    let dir = pulse_to_dir(*pulse);
                    let steps = accel_steps(self.last_encoder_tick[1]);
                    self.last_encoder_tick[1] = 0;
                    for _ in 0..steps {
                        encoder_cc(preset, 1, dir, &mut self.encoder_values[1]);
                    }
                    // Send only the final value
                    if let Some(enc) = preset.encoders.get(1) {
                        if let pedalboard_protocol::config::EncoderAction::Cc {
                            cc, channel, ..
                        } = &enc.action
                        {
                            push_midi(
                                &mut midi,
                                &MidiMessage {
                                    data: [0xB0 | (channel - 1), *cc as u8, self.encoder_values[1]],
                                    len: 3,
                                },
                            );
                        }
                        display
                            .push(DisplayEvent::EncoderOverlay {
                                side: DisplaySide::R,
                                label: enc.label.clone(),
                                value: self.encoder_values[1],
                            })
                            .ok();
                    }
                    led_dirty = true;
                }
                InputEvent::ExpressionPedalA(raw_adc) => {
                    // ADC trim: pedal hardware doesn't reach full 4095
                    // TODO: make configurable (per-board calibration)
                    let adc = (*raw_adc).min(ADC_MAX_TRIMMED);
                    if let Some(msg) = analog_cc(preset, 0, adc, ADC_MAX_TRIMMED) {
                        display
                            .push(DisplayEvent::AnalogOverlay {
                                side: DisplaySide::L,
                                label: preset
                                    .analog
                                    .first()
                                    .map(|a| a.label.clone())
                                    .unwrap_or_default(),
                                value: msg.data[2],
                            })
                            .ok();
                        push_midi(&mut midi, &msg);
                    }
                }
                InputEvent::ExpressionPedalB(raw_adc) => {
                    let adc = (*raw_adc).min(ADC_MAX_TRIMMED);
                    if let Some(msg) = analog_cc(preset, 1, adc, ADC_MAX_TRIMMED) {
                        display
                            .push(DisplayEvent::AnalogOverlay {
                                side: DisplaySide::R,
                                label: preset
                                    .analog
                                    .get(1)
                                    .map(|a| a.label.clone())
                                    .unwrap_or_default(),
                                value: msg.data[2],
                            })
                            .ok();
                        push_midi(&mut midi, &msg);
                    }
                }
                _ => {}
            }
        }
        HandleResult {
            midi,
            system,
            display,
            led_dirty,
        }
    }

    /// Compute LED animations for all 8 rings based on current state + preset config.
    /// Order: [A, B, C, D, E, F, Vol, Gain]
    pub fn led_state(&self, preset: &Preset) -> LedAnimations {
        let mut anims = [Animation::Off; 8];

        // Button rings A-F
        for (i, anim) in anims.iter_mut().enumerate().take(NUM_BUTTONS) {
            if let Some(btn) = preset.buttons.get(i) {
                let color = if self.button_active[i] {
                    color_to_rgb(&btn.color.on)
                } else if btn.color.off == Color::Off {
                    // Dim the on-color as idle indicator
                    let on = color_to_rgb(&btn.color.on);
                    RGB8::new(on.r / 6, on.g / 6, on.b / 6)
                } else {
                    color_to_rgb(&btn.color.off)
                };
                *anim = if color == RGB8::default() {
                    Animation::Off
                } else {
                    Animation::On(color)
                };
            }
        }

        // Encoder rings: heatmap from current value
        let fill_vol = ((self.encoder_values[0] as u16 * 12) / 127).min(12) as u8;
        anims[6] = Animation::Heatmap(fill_vol);
        let fill_gain = ((self.encoder_values[1] as u16 * 12) / 127).min(12) as u8;
        anims[7] = Animation::Heatmap(fill_gain);

        anims
    }
}

fn execute_actions(
    actions: &heapless::Vec<Action, { pedalboard_protocol::config::MAX_ACTIONS }>,
    cycle_values: &heapless::Vec<u8, { pedalboard_protocol::config::MAX_CYCLE_VALUES }>,
    midi: &mut heapless::Vec<([u8; 3], usize), 8>,
    system: &mut heapless::Vec<SystemAction, 2>,
    cycle_index: &mut u8,
) {
    for action in actions {
        match action {
            Action::PresetNext => {
                system.push(SystemAction::PresetNext).ok();
            }
            Action::PresetPrev => {
                system.push(SystemAction::PresetPrev).ok();
            }
            Action::CcCycle {
                cc,
                channel,
                reverse,
            } => {
                if !cycle_values.is_empty() {
                    let idx = (*cycle_index as usize) % cycle_values.len();
                    let value = cycle_values[idx];
                    push_midi(
                        midi,
                        &MidiMessage {
                            data: [0xB0 | (channel - 1), *cc, value],
                            len: 3,
                        },
                    );
                    if *reverse {
                        *cycle_index = if *cycle_index == 0 {
                            (cycle_values.len() - 1) as u8
                        } else {
                            *cycle_index - 1
                        };
                    } else {
                        *cycle_index = ((*cycle_index as usize + 1) % cycle_values.len()) as u8;
                    }
                }
            }
            _ => {
                if let Some(msg) = action_to_midi(action) {
                    push_midi(midi, &msg);
                }
            }
        }
    }
}

/// Acceleration: faster turning = bigger steps.
/// ticks_since_last is in ms (1ms poll rate).
fn accel_steps(ticks_since_last: u16) -> u8 {
    if ticks_since_last < 20 {
        8 // very fast
    } else if ticks_since_last < 50 {
        4 // fast
    } else if ticks_since_last < 100 {
        2 // moderate
    } else {
        1 // slow/normal
    }
}

fn color_to_rgb(c: &Color) -> RGB8 {
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
                cycle_values: Vec::new(),
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
                cycle_values: Vec::new(),
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

    #[test]
    fn encoder_emits_display_event() {
        let preset = make_test_preset();
        let mut handler = PeHandler::new();
        handler.encoder_values[0] = 64;
        let r = handler.handle_events(&preset, &[InputEvent::Vol(Pulse::Clockwise)]);
        assert_eq!(r.display.len(), 1);
        match &r.display[0] {
            DisplayEvent::EncoderOverlay { side, label, value } => {
                assert_eq!(*side, DisplaySide::L);
                assert_eq!(label.as_str(), "Vol");
                assert_eq!(*value, 65);
            }
            _ => panic!("expected EncoderOverlay"),
        }
    }

    // --- LED state tests ---

    fn make_led_preset() -> Preset {
        use pedalboard_protocol::config::*;
        let mut buttons: heapless::Vec<ButtonConfig, MAX_BUTTONS> = heapless::Vec::new();
        // Button A: momentary, green on, off when inactive
        buttons
            .push(ButtonConfig {
                label: Label::new(),
                color: LedConfig {
                    on: Color::Green,
                    off: Color::Off,
                },
                mode: ButtonMode::Momentary,
                on_press: heapless::Vec::new(),
                on_release: heapless::Vec::new(),
                on_long_press: heapless::Vec::new(),
                cycle_values: heapless::Vec::new(),
            })
            .ok();
        // Button B: toggle, blue on, red off
        buttons
            .push(ButtonConfig {
                label: Label::new(),
                color: LedConfig {
                    on: Color::Blue,
                    off: Color::Red,
                },
                mode: ButtonMode::Toggle,
                on_press: heapless::Vec::new(),
                on_release: heapless::Vec::new(),
                on_long_press: heapless::Vec::new(),
                cycle_values: heapless::Vec::new(),
            })
            .ok();
        Preset {
            name: Label::try_from("LED").unwrap(),
            buttons,
            encoders: heapless::Vec::new(),
            analog: heapless::Vec::new(),
        }
    }

    #[test]
    fn led_state_off_by_default() {
        let preset = make_led_preset();
        let handler = PeHandler::new();
        let leds = handler.led_state(&preset);
        // Button A: off color = Off, on = Green → dim green idle indicator
        assert!(matches!(leds[0], Animation::On(c) if c == RGB8::new(0, 255/6, 0)));
        // Button B: off color = Red → Animation::On(red)
        assert!(matches!(leds[1], Animation::On(c) if c == RGB8::new(255, 0, 0)));
    }

    #[test]
    fn led_state_momentary_on_while_pressed() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[0], Animation::On(c) if c == RGB8::new(0, 255, 0)));
    }

    #[test]
    fn led_state_momentary_off_after_release() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
        handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Deactivate)]);
        let leds = handler.led_state(&preset);
        // Returns to dim idle (on=Green, off=Off → dim green)
        assert!(matches!(leds[0], Animation::On(c) if c == RGB8::new(0, 255/6, 0)));
    }

    #[test]
    fn led_state_toggle_alternates() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        // First press → toggle on (blue)
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[1], Animation::On(c) if c == RGB8::new(0, 0, 255)));
        // Release doesn't change toggle state
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[1], Animation::On(c) if c == RGB8::new(0, 0, 255)));
        // Second press → toggle off (red)
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        let leds = handler.led_state(&preset);
        assert!(matches!(leds[1], Animation::On(c) if c == RGB8::new(255, 0, 0)));
    }

    #[test]
    fn led_state_encoder_heatmap() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        handler.encoder_values[0] = 64;
        handler.encoder_values[1] = 127;
        let leds = handler.led_state(&preset);
        // Vol ring at ~midpoint
        assert!(matches!(leds[6], Animation::Heatmap(6)));
        // Gain ring at max
        assert!(matches!(leds[7], Animation::Heatmap(12)));
    }

    #[test]
    fn led_dirty_on_button_press() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        let r = handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
        assert!(r.led_dirty);
    }

    #[test]
    fn led_not_dirty_when_idle() {
        let preset = make_led_preset();
        let mut handler = PeHandler::new();
        let r = handler.handle_events(&preset, &[]);
        assert!(!r.led_dirty);
    }

    // --- CcCycle tests ---

    fn make_cycle_preset() -> Preset {
        use pedalboard_protocol::config::*;
        let mut buttons: heapless::Vec<ButtonConfig, MAX_BUTTONS> = heapless::Vec::new();
        let mut on_press: heapless::Vec<Action, MAX_ACTIONS> = heapless::Vec::new();
        let mut values: heapless::Vec<u8, MAX_CYCLE_VALUES> = heapless::Vec::new();
        for &v in &[0u8, 8, 17, 26, 35] {
            values.push(v).ok();
        }
        on_press
            .push(Action::CcCycle {
                cc: 8,
                channel: 1,
                reverse: false,
            })
            .ok();
        buttons
            .push(ButtonConfig {
                label: Label::try_from("Kit+").unwrap(),
                color: LedConfig::default(),
                mode: ButtonMode::Momentary,
                on_press,
                on_release: heapless::Vec::new(),
                on_long_press: heapless::Vec::new(),
                cycle_values: values.clone(),
            })
            .ok();
        // Button B: reverse cycle
        let mut on_press_b: heapless::Vec<Action, MAX_ACTIONS> = heapless::Vec::new();
        on_press_b
            .push(Action::CcCycle {
                cc: 8,
                channel: 1,
                reverse: true,
            })
            .ok();
        buttons
            .push(ButtonConfig {
                label: Label::try_from("Kit-").unwrap(),
                color: LedConfig::default(),
                mode: ButtonMode::Momentary,
                on_press: on_press_b,
                on_release: heapless::Vec::new(),
                on_long_press: heapless::Vec::new(),
                cycle_values: values,
            })
            .ok();
        Preset {
            name: Label::try_from("Cycle").unwrap(),
            buttons,
            encoders: heapless::Vec::new(),
            analog: heapless::Vec::new(),
        }
    }

    #[test]
    fn cc_cycle_forward() {
        let preset = make_cycle_preset();
        let mut handler = PeHandler::new();
        // First press → value 0
        let r = handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
        assert_eq!(r.midi[0].0, [0xB0, 8, 0]);
        handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Deactivate)]);
        // Second press → value 8
        let r = handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
        assert_eq!(r.midi[0].0, [0xB0, 8, 8]);
        handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Deactivate)]);
        // Third → 17
        let r = handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
        assert_eq!(r.midi[0].0, [0xB0, 8, 17]);
    }

    #[test]
    fn cc_cycle_wraps() {
        let preset = make_cycle_preset();
        let mut handler = PeHandler::new();
        // Press 5 times (list has 5 values), then 6th wraps to 0
        for _ in 0..5 {
            handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
            handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Deactivate)]);
        }
        let r = handler.handle_events(&preset, &[InputEvent::ButtonA(Edge::Activate)]);
        assert_eq!(r.midi[0].0, [0xB0, 8, 0]); // wrapped
    }

    #[test]
    fn cc_cycle_reverse() {
        let preset = make_cycle_preset();
        let mut handler = PeHandler::new();
        // First press reverse → starts at index 0, sends value 0, then index wraps to 4
        let r = handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        assert_eq!(r.midi[0].0, [0xB0, 8, 0]);
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
        // Second press → index 4 → value 35
        let r = handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        assert_eq!(r.midi[0].0, [0xB0, 8, 35]);
        handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Deactivate)]);
        // Third → index 3 → value 26
        let r = handler.handle_events(&preset, &[InputEvent::ButtonB(Edge::Activate)]);
        assert_eq!(r.midi[0].0, [0xB0, 8, 26]);
    }
}

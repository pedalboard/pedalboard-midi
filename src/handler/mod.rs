mod live_effect;
mod live_looper;
mod setup_looper;

use crate::hmi::inputs::{Edge::Activate, InputEvent};
use defmt::*;
use heapless::Vec;
use midi_types::{MidiMessage, Note};
use smart_leds::colors::*;

use crate::hmi::leds::{
    Animation::{Flash, On},
    Led, Leds,
};

type MidiMessageVec = Vec<MidiMessage, 8>;

#[derive(Debug)]
pub struct MidiMessages(MidiMessageVec);

impl MidiMessages {
    pub fn push(&mut self, a: MidiMessage) {
        if self.0.push(a).is_err() {
            error!("failed pushing midi message")
        };
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn none() -> Self {
        MidiMessages(Vec::new())
    }

    pub fn messages(self) -> MidiMessageVec {
        self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub struct Actions {
    pub midi_messages: MidiMessages,
}

impl Actions {
    fn new(midi_messages: MidiMessages) -> Self {
        Actions { midi_messages }
    }
    fn none() -> Self {
        Actions::new(MidiMessages::none())
    }
}
pub trait Handler {
    fn handle_human_input(&mut self, e: InputEvent) -> Actions;
    fn leds(&mut self) -> &mut Leds;
}

enum HandlerEnum {
    LiveEffect(self::live_effect::LiveEffect),
    LiveLooper(self::live_looper::LiveLooper),
    SetupLooper(self::setup_looper::SetupLooper),
}

impl Handler for HandlerEnum {
    fn handle_human_input(&mut self, e: InputEvent) -> Actions {
        match self {
            HandlerEnum::LiveEffect(h) => h.handle_human_input(e),
            HandlerEnum::LiveLooper(h) => h.handle_human_input(e),
            HandlerEnum::SetupLooper(h) => h.handle_human_input(e),
        }
    }
    fn leds(&mut self) -> &mut Leds {
        match self {
            HandlerEnum::LiveEffect(h) => h.leds(),
            HandlerEnum::LiveLooper(h) => h.leds(),
            HandlerEnum::SetupLooper(h) => h.leds(),
        }
    }
}

pub struct Handlers {
    handlers: [HandlerEnum; 3],
    current: usize,
}

impl Handlers {
    pub fn new() -> Self {
        Handlers {
            handlers: [
                HandlerEnum::LiveEffect(self::live_effect::LiveEffect::new()),
                HandlerEnum::LiveLooper(self::live_looper::LiveLooper::new()),
                HandlerEnum::SetupLooper(self::setup_looper::SetupLooper::new()),
            ],
            current: 0,
        }
    }

    pub fn handle_human_input(&mut self, event: InputEvent) -> Actions {
        let actions = match event {
            InputEvent::VolButton(Activate) => {
                self.current += 1;
                if self.current == self.handlers.len() {
                    self.current = 0
                }
                Actions::none()
            }
            _ => self.handler().handle_human_input(event),
        };
        if !actions.midi_messages.is_empty() {
            // MIDI-out indicator
            self.leds().set(Flash(DARK_GREEN), Led::Mon);
        }

        actions
    }
    pub fn process_midi_input(&mut self, m: MidiMessage) {
        match m {
            // see https://github.com/pedalboard/db-meter.lv2
            MidiMessage::NoteOff(_, Note::C1, vel) => {
                let v: u8 = vel.into();
                let lufs = -(v as f32);
                debug!("loudness {}", lufs);
                let color = crate::loudness::loudness_color(lufs);
                self.leds()
                    .set_ledring(super::hmi::ledring::Animation::Loudness(lufs));
                self.leds().set(On(color), Led::L48V);
            }
            _ => {
                // MIDI-in indicator
                self.leds().set(Flash(DARK_BLUE), Led::Mon);
            }
        }
    }

    fn handler(&mut self) -> &mut HandlerEnum {
        &mut self.handlers[self.current]
    }

    pub fn leds(&mut self) -> &mut Leds {
        self.handler().leds()
    }
}

impl Default for Handlers {
    fn default() -> Self {
        Self::new()
    }
}

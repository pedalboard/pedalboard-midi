pub mod dispatch;
pub mod live_effect;
pub mod live_looper;
pub mod setup_looper;

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
        self.0.push(a).unwrap();
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

const MAX_HANDLERS: usize = 8;
pub type HandlerVec<H> = Vec<H, MAX_HANDLERS>;

pub struct Handlers<H: Handler> {
    handlers: HandlerVec<H>,
    current: usize,
}

/// The router (dispatcher) for human input and midi input
impl<H> Handlers<H>
where
    H: Handler,
{
    pub fn new(handlers: Vec<H, MAX_HANDLERS>) -> Self {
        Handlers {
            handlers,
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

    fn handler(&mut self) -> &mut H {
        &mut self.handlers[self.current]
    }

    pub fn leds(&mut self) -> &mut Leds {
        self.handler().leds()
    }
}

/// Construct empty Handlers
impl<H> Default for Handlers<H>
where
    H: Handler,
{
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

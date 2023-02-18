mod plethora;
mod rc500;

use heapless::Vec;
use midi_types::MidiMessage;

use crate::hmi::Edge::{Activate, Deactivate};

use self::plethora::{Plethora, PlethoraEvent};
use self::rc500::{RC500Event, RC500};
use crate::hmi::InputEvent;

pub type MidiMessages = Vec<MidiMessage, 8>;

const NO_MESSAGE: MidiMessages = Vec::new();

pub enum Direction {
    Up,
    Down,
}
pub struct Devices {
    rc500: RC500,
    plethora: Plethora,
}

impl Devices {
    pub fn new() -> Self {
        Devices {
            rc500: RC500::new(),
            plethora: Plethora {},
        }
    }

    pub fn map(&mut self, event: crate::hmi::InputEvent) -> MidiMessages {
        match event {
            InputEvent::ButtonA(e) => match e {
                Activate => self.plethora(PlethoraEvent::Board(Direction::Up)),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ButtonB(e) => match e {
                Activate => self.rc500(RC500Event::Mem(Direction::Up)),
                Deactivate => NO_MESSAGE,
            },
            _ => NO_MESSAGE,
        }
    }

    fn plethora(&mut self, event: PlethoraEvent) -> MidiMessages {
        self.plethora.midi_messages(event)
    }

    fn rc500(&mut self, event: RC500Event) -> MidiMessages {
        self.rc500.midi_messages(event)
    }
}

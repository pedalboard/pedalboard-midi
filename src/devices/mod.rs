mod plethora;
mod rc500;

use heapless::Vec;
use midi_types::MidiMessage;

use crate::hmi::Edge::{Activate, Deactivate};

use self::plethora::Plethora;
use self::rc500::{RC500Event, RC500};
use crate::hmi::InputEvent;

pub type MidiMessages = Vec<MidiMessage, 8>;

pub struct Devices {
    rc500: RC500,
}

impl Devices {
    pub fn new() -> Self {
        Devices {
            rc500: RC500::new(),
        }
    }

    pub fn map(&mut self, event: crate::hmi::InputEvent) -> MidiMessages {
        match event {
            InputEvent::ButtonA(e) => match e {
                Activate => self.plethora(Plethora::BoardUp),
                Deactivate => Vec::new(),
            },
            InputEvent::ButtonB(e) => match e {
                Activate => self.rc500(RC500Event::Mem(rc500::Direction::Up)),
                Deactivate => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    fn plethora(&mut self, event: Plethora) -> MidiMessages {
        event.midi_messages()
    }

    fn rc500(&mut self, event: RC500Event) -> MidiMessages {
        self.rc500.midi_messages(event)
    }
}

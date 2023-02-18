mod plethora;
mod rc500;

use heapless::Vec;
use midi_types::MidiMessage;

use self::plethora::Plethora;
use self::rc500::{RC500Event, RC500};

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
            crate::hmi::InputEvent::ButtonA(_) => self.plethora(Plethora::BoardUp),
            crate::hmi::InputEvent::ButtonB(_) => self.rc500(RC500Event::Mem(rc500::Direction::Up)),
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

mod plethora;
mod rc500;

use heapless::Vec;
use midi_types::MidiMessage;

use crate::hmi::inputs::{
    Edge::{Activate, Deactivate},
    InputEvent,
};

use self::plethora::{Plethora, PlethoraEvent};
use self::rc500::{RC500Event, RC500};

pub type MidiMessages = Vec<MidiMessage, 8>;

const NO_MESSAGE: MidiMessages = Vec::new();

pub enum Direction {
    Up,
    Down,
}

pub struct Devices {
    rc500: RC500,
    plethora: Plethora,
    current: Modes,
}

pub enum Modes {
    LiveEffect,
    LiveLooper,
    SetupLooper,
}

impl Devices {
    pub fn new() -> Self {
        Devices {
            rc500: RC500::new(),
            plethora: Plethora {},
            current: Modes::LiveEffect,
        }
    }

    pub fn map(&mut self, event: InputEvent) -> MidiMessages {
        match event {
            InputEvent::GainButton(e) => match e {
                Activate => {
                    match self.current {
                        Modes::LiveEffect => self.current = Modes::LiveLooper,
                        Modes::LiveLooper => self.current = Modes::SetupLooper,
                        Modes::SetupLooper => self.current = Modes::LiveEffect,
                    };
                    NO_MESSAGE
                }
                Deactivate => NO_MESSAGE,
            },
            _ => match self.current {
                Modes::LiveEffect => self.map_live_effect(event),
                Modes::LiveLooper => self.map_live_looper(event),
                Modes::SetupLooper => self.map_setup_looper(event),
            },
        }
    }

    fn map_live_effect(&mut self, event: InputEvent) -> MidiMessages {
        match event {
            InputEvent::ButtonA(e) => match e {
                Activate => self.plethora(PlethoraEvent::GoToBoard(1)),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ButtonB(e) => match e {
                Activate => self.plethora(PlethoraEvent::GoToBoard(2)),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ButtonC(e) => match e {
                Activate => self.plethora(PlethoraEvent::GoToBoard(3)),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ButtonD(e) => match e {
                Activate => self.plethora(PlethoraEvent::Board(Direction::Up)),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ButtonE(e) => match e {
                Activate => self.plethora(PlethoraEvent::Board(Direction::Down)),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ButtonF(e) => match e {
                Activate => self.plethora(PlethoraEvent::GoToBoard(4)),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ExpessionPedal(val) => self.plethora(PlethoraEvent::HotKnob(3, val)),
            _ => NO_MESSAGE,
        }
    }
    fn map_live_looper(&mut self, event: InputEvent) -> MidiMessages {
        match event {
            InputEvent::ButtonA(e) => match e {
                Activate => self.rc500(RC500Event::ToggleRhythm()),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ButtonB(e) => match e {
                Activate => self.rc500(RC500Event::RhythmVariation()),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ButtonD(e) => match e {
                Activate => self.rc500(RC500Event::Mem(Direction::Up)),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ButtonE(e) => match e {
                Activate => self.rc500(RC500Event::Mem(Direction::Down)),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ButtonF(e) => match e {
                Activate => self.rc500(RC500Event::ClearCurrent()),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ExpessionPedal(val) => self.rc500(RC500Event::CurrentChannelLevel(val)),
            _ => NO_MESSAGE,
        }
    }
    fn map_setup_looper(&mut self, event: InputEvent) -> MidiMessages {
        match event {
            InputEvent::ButtonA(e) => match e {
                Activate => self.rc500(RC500Event::RhythmPattern(Direction::Up)),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ButtonB(e) => match e {
                Activate => self.rc500(RC500Event::RhythmPattern(Direction::Down)),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ButtonD(e) => match e {
                Activate => self.rc500(RC500Event::DrumKit(Direction::Up)),
                Deactivate => NO_MESSAGE,
            },
            InputEvent::ButtonE(e) => match e {
                Activate => self.rc500(RC500Event::DrumKit(Direction::Down)),
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

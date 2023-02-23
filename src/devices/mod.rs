mod plethora;
mod rc500;

use heapless::Vec;
use midi_types::MidiMessage;
use smart_leds::RGB8;

use crate::hmi::inputs::{
    Edge::{Activate, Deactivate},
    InputEvent,
};

use crate::hmi::leds::{Animation, Led};

use self::plethora::{Plethora, PlethoraEvent};
use self::rc500::{RC500Event, RC500};

pub type MidiMessages = Vec<MidiMessage, 8>;
pub type Animations = Vec<Animation, 8>;

const NO_MESSAGE: MidiMessages = Vec::new();
const NO_ANIMATIONS: Animations = Vec::new();

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

    pub fn map(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::GainButton(e) => match e {
                Activate => {
                    let mode_color = match self.current {
                        Modes::LiveEffect => {
                            self.current = Modes::LiveLooper;
                            RGB8::new(255, 0, 0)
                        }
                        Modes::LiveLooper => {
                            self.current = Modes::SetupLooper;
                            RGB8::new(127, 0, 0)
                        }
                        Modes::SetupLooper => {
                            self.current = Modes::LiveEffect;
                            RGB8::new(255, 255, 255)
                        }
                    };
                    let mut animations: Animations = Vec::new();
                    animations
                        .push(Animation::On(Led::Mode, mode_color))
                        .unwrap();
                    Actions::new(NO_MESSAGE, animations)
                }
                Deactivate => Actions::default(),
            },
            _ => match self.current {
                Modes::LiveEffect => self.map_live_effect(event),
                Modes::LiveLooper => self.map_live_looper(event),
                Modes::SetupLooper => self.map_setup_looper(event),
            },
        }
    }

    fn map_live_effect(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(e) => match e {
                Activate => Actions::new(self.plethora(PlethoraEvent::GoToBoard(1)), NO_ANIMATIONS),
                Deactivate => Actions::default(),
            },
            InputEvent::ButtonB(e) => match e {
                Activate => Actions::new(self.plethora(PlethoraEvent::GoToBoard(2)), NO_ANIMATIONS),
                Deactivate => Actions::default(),
            },
            InputEvent::ButtonC(e) => match e {
                Activate => Actions::new(self.plethora(PlethoraEvent::GoToBoard(3)), NO_ANIMATIONS),
                Deactivate => Actions::default(),
            },
            InputEvent::ButtonD(e) => match e {
                Activate => Actions::new(
                    self.plethora(PlethoraEvent::Board(Direction::Up)),
                    NO_ANIMATIONS,
                ),
                Deactivate => Actions::default(),
            },
            InputEvent::ButtonE(e) => match e {
                Activate => Actions::new(
                    self.plethora(PlethoraEvent::Board(Direction::Down)),
                    NO_ANIMATIONS,
                ),
                Deactivate => Actions::default(),
            },
            InputEvent::ButtonF(e) => match e {
                Activate => Actions::new(self.plethora(PlethoraEvent::GoToBoard(4)), NO_ANIMATIONS),
                Deactivate => Actions::default(),
            },
            InputEvent::ExpessionPedal(val) => {
                Actions::new(self.plethora(PlethoraEvent::HotKnob(3, val)), NO_ANIMATIONS)
            }
            _ => Actions::default(),
        }
    }
    fn map_live_looper(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(e) => match e {
                Activate => Actions::new(self.rc500(RC500Event::ToggleRhythm()), NO_ANIMATIONS),
                Deactivate => Actions::default(),
            },
            InputEvent::ButtonB(e) => match e {
                Activate => Actions::new(self.rc500(RC500Event::RhythmVariation()), NO_ANIMATIONS),
                Deactivate => Actions::default(),
            },
            InputEvent::ButtonD(e) => match e {
                Activate => Actions::new(self.rc500(RC500Event::Mem(Direction::Up)), NO_ANIMATIONS),
                Deactivate => Actions::default(),
            },
            InputEvent::ButtonE(e) => match e {
                Activate => {
                    Actions::new(self.rc500(RC500Event::Mem(Direction::Down)), NO_ANIMATIONS)
                }
                Deactivate => Actions::default(),
            },
            InputEvent::ButtonF(e) => match e {
                Activate => Actions::new(self.rc500(RC500Event::ClearCurrent()), NO_ANIMATIONS),
                Deactivate => Actions::default(),
            },
            InputEvent::ExpessionPedal(val) => Actions::new(
                self.rc500(RC500Event::CurrentChannelLevel(val)),
                NO_ANIMATIONS,
            ),
            _ => Actions::default(),
        }
    }
    fn map_setup_looper(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(e) => match e {
                Activate => Actions::new(
                    self.rc500(RC500Event::RhythmPattern(Direction::Up)),
                    NO_ANIMATIONS,
                ),
                Deactivate => Actions::default(),
            },
            InputEvent::ButtonB(e) => match e {
                Activate => Actions::new(
                    self.rc500(RC500Event::RhythmPattern(Direction::Down)),
                    NO_ANIMATIONS,
                ),
                Deactivate => Actions::default(),
            },
            InputEvent::ButtonD(e) => match e {
                Activate => Actions::new(
                    self.rc500(RC500Event::DrumKit(Direction::Up)),
                    NO_ANIMATIONS,
                ),
                Deactivate => Actions::default(),
            },
            InputEvent::ButtonE(e) => match e {
                Activate => Actions::new(
                    self.rc500(RC500Event::DrumKit(Direction::Down)),
                    NO_ANIMATIONS,
                ),
                Deactivate => Actions::default(),
            },
            _ => Actions::default(),
        }
    }

    fn plethora(&mut self, event: PlethoraEvent) -> MidiMessages {
        self.plethora.midi_messages(event)
    }

    fn rc500(&mut self, event: RC500Event) -> MidiMessages {
        self.rc500.midi_messages(event)
    }
}

pub struct Actions {
    pub midi_messages: MidiMessages,
    pub animations: Animations,
}

impl Actions {
    fn new(m: MidiMessages, a: Animations) -> Self {
        Actions {
            midi_messages: m,
            animations: a,
        }
    }
    fn default() -> Self {
        Actions::new(NO_MESSAGE, NO_ANIMATIONS)
    }
}

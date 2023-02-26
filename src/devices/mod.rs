mod pedalboardaudio;
mod plethora;
mod rc500;

use crate::hmi::inputs::{
    Edge::{Activate, Deactivate},
    InputEvent,
};
use heapless::Vec;
use midi_types::MidiMessage;
use smart_leds::{
    colors::{BLUE, GREEN, ORANGE, RED, SEA_GREEN, VIOLET, WHITE},
    RGB8,
};

use crate::hmi::leds::{
    Animation::{Off, On},
    Led,
};

use self::pedalboardaudio::{PAEvent, PedalboardAudio};
use self::plethora::{Plethora, PlethoraEvent};
use self::rc500::{RC500Event, RC500};

pub type MidiMessages = Vec<MidiMessage, 8>;
pub type Animations = Vec<crate::hmi::leds::Animation, 8>;

const NO_MESSAGE: MidiMessages = Vec::new();
const NO_ANIMATIONS: Animations = Vec::new();

pub enum Direction {
    Up,
    Down,
}

pub struct Devices {
    rc500: RC500,
    plethora: Plethora,
    audio: PedalboardAudio,
    mode: Modes,
}

pub enum Modes {
    LiveEffect,
    LiveLooper,
    SetupLooper,
}

impl Devices {
    pub fn new() -> Self {
        Devices {
            rc500: RC500::default(),
            audio: PedalboardAudio::default(),
            plethora: Plethora {},
            mode: Modes::LiveEffect,
        }
    }
    pub fn map(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::GainButton(e) => match e {
                Activate => {
                    let mode_color = match self.mode {
                        Modes::LiveEffect => {
                            self.mode = Modes::LiveLooper;
                            RED
                        }
                        Modes::LiveLooper => {
                            self.mode = Modes::SetupLooper;
                            ORANGE
                        }
                        Modes::SetupLooper => {
                            self.mode = Modes::LiveEffect;
                            WHITE
                        }
                    };
                    let mut animations: Animations = Vec::new();
                    animations.push(On(Led::Mode, mode_color)).unwrap();
                    Actions::new(NO_MESSAGE, animations)
                }
                Deactivate => Actions::default(),
            },
            _ => match self.mode {
                Modes::LiveEffect => self.map_live_effect(event),
                Modes::LiveLooper => self.map_live_looper(event),
                Modes::SetupLooper => self.map_setup_looper(event),
            },
        }
    }

    fn map_live_effect(&mut self, event: InputEvent) -> Actions {
        let mut animations: Animations = Vec::new();
        animations.push(Off(Led::A)).unwrap();
        animations.push(Off(Led::B)).unwrap();
        animations.push(Off(Led::C)).unwrap();
        animations.push(Off(Led::F)).unwrap();
        match event {
            InputEvent::ButtonA(Activate) => {
                animations.push(On(Led::A, BLUE)).unwrap();
                Actions::new(self.plethora(PlethoraEvent::GoToBoard(1)), animations)
            }
            InputEvent::ButtonB(Activate) => {
                animations.push(On(Led::B, SEA_GREEN)).unwrap();
                Actions::new(self.plethora(PlethoraEvent::GoToBoard(2)), animations)
            }
            InputEvent::ButtonC(Activate) => {
                animations.push(On(Led::C, GREEN)).unwrap();
                Actions::new(self.plethora(PlethoraEvent::GoToBoard(3)), animations)
            }
            InputEvent::ButtonF(Activate) => {
                animations.push(On(Led::F, VIOLET)).unwrap();
                Actions::new(self.plethora(PlethoraEvent::GoToBoard(4)), animations)
            }
            InputEvent::ButtonD(Activate) => Actions::new(
                self.plethora(PlethoraEvent::Board(Direction::Up)),
                animations,
            ),
            InputEvent::ButtonE(Activate) => Actions::new(
                self.plethora(PlethoraEvent::Board(Direction::Down)),
                animations,
            ),
            InputEvent::ExpessionPedal(val) => {
                animations.clear();
                let v: u8 = val.into();
                let c = colorous::REDS.eval_rational(v as usize, 127);
                let color = RGB8::new(c.r, c.g, c.b);
                animations.push(On(Led::Clip, color)).unwrap();
                Actions::new(self.audio(PAEvent::OutputLevel(val)), animations)
            }
            _ => Actions::default(),
        }
    }
    fn map_live_looper(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(Activate) => {
                Actions::new(self.rc500(RC500Event::ToggleRhythm()), NO_ANIMATIONS)
            }
            InputEvent::ButtonB(Activate) => {
                Actions::new(self.rc500(RC500Event::RhythmVariation()), NO_ANIMATIONS)
            }
            InputEvent::ButtonD(Activate) => {
                Actions::new(self.rc500(RC500Event::Mem(Direction::Up)), NO_ANIMATIONS)
            }
            InputEvent::ButtonE(Activate) => {
                Actions::new(self.rc500(RC500Event::Mem(Direction::Down)), NO_ANIMATIONS)
            }
            InputEvent::ButtonF(Activate) => {
                Actions::new(self.rc500(RC500Event::ClearCurrent()), NO_ANIMATIONS)
            }
            InputEvent::ExpessionPedal(val) => Actions::new(
                self.rc500(RC500Event::CurrentChannelLevel(val)),
                NO_ANIMATIONS,
            ),
            _ => Actions::default(),
        }
    }
    fn map_setup_looper(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(Activate) => Actions::new(
                self.rc500(RC500Event::RhythmPattern(Direction::Up)),
                NO_ANIMATIONS,
            ),
            InputEvent::ButtonB(Activate) => Actions::new(
                self.rc500(RC500Event::RhythmPattern(Direction::Down)),
                NO_ANIMATIONS,
            ),
            InputEvent::ButtonD(Activate) => Actions::new(
                self.rc500(RC500Event::DrumKit(Direction::Up)),
                NO_ANIMATIONS,
            ),
            InputEvent::ButtonE(Activate) => Actions::new(
                self.rc500(RC500Event::DrumKit(Direction::Down)),
                NO_ANIMATIONS,
            ),
            _ => Actions::default(),
        }
    }

    fn plethora(&mut self, event: PlethoraEvent) -> MidiMessages {
        self.plethora.midi_messages(event)
    }

    fn rc500(&mut self, event: RC500Event) -> MidiMessages {
        self.rc500.midi_messages(event)
    }
    fn audio(&mut self, event: PAEvent) -> MidiMessages {
        self.audio.midi_messages(event)
    }
}

impl Default for Devices {
    fn default() -> Self {
        Self::new()
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

mod pedalboardaudio;
mod plethora;
mod rc500;

use crate::hmi::inputs::{
    Edge::{Activate, Deactivate},
    InputEvent,
};
use defmt::error;
use heapless::Vec;
use midi_types::MidiMessage;
use smart_leds::{
    colors::{BLUE, GREEN, ORANGE, RED, SEA_GREEN, VIOLET, WHITE},
    RGB8,
};

use crate::hmi::leds::{
    Animation::{Off, On},
    Animations, Led,
};

use self::pedalboardaudio::{PAAction, PedalboardAudio};
use self::plethora::{Plethora, PlethoraAction};
use self::rc500::{RC500Action, RC500};

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

pub enum Direction {
    Up,
    Down,
}

pub struct Devices {
    rc500: RC500,
    plethora: Plethora,
    audio: PedalboardAudio,
    modes: [Mode; 3],
    current_mode: usize,
}

pub enum Mode {
    LiveEffect,
    LiveLooper,
    SetupLooper,
}

impl Devices {
    pub fn new() -> Self {
        Devices {
            modes: [Mode::LiveEffect, Mode::LiveLooper, Mode::SetupLooper],
            current_mode: 0,
            rc500: RC500::default(),
            audio: PedalboardAudio::default(),
            plethora: Plethora {},
        }
    }
    pub fn map(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::GainButton(e) => match e {
                Activate => {
                    self.current_mode += 1;
                    if self.current_mode == self.modes.len() {
                        self.current_mode = 0
                    }
                    let mode_color = match self.modes[self.current_mode] {
                        Mode::LiveEffect => WHITE,
                        Mode::LiveLooper => RED,
                        Mode::SetupLooper => ORANGE,
                    };
                    let mut animations = Animations::none();
                    animations.push(On(Led::Mode, mode_color));
                    animations.all_button_leds_off();
                    Actions::new(MidiMessages::none(), animations)
                }
                Deactivate => Actions::default(),
            },
            _ => match self.modes[self.current_mode] {
                Mode::LiveEffect => self.map_live_effect(event),
                Mode::LiveLooper => self.map_live_looper(event),
                Mode::SetupLooper => self.map_setup_looper(event),
            },
        }
    }

    fn map_live_effect(&mut self, event: InputEvent) -> Actions {
        let mut animations = Animations::none();
        animations.push(Off(Led::A));
        animations.push(Off(Led::B));
        animations.push(Off(Led::C));
        animations.push(Off(Led::F));
        match event {
            InputEvent::ButtonA(Activate) => {
                animations.push(On(Led::A, BLUE));
                Actions::new(self.plethora(PlethoraAction::GoToBoard(1)), animations)
            }
            InputEvent::ButtonB(Activate) => {
                animations.push(On(Led::B, SEA_GREEN));
                Actions::new(self.plethora(PlethoraAction::GoToBoard(2)), animations)
            }
            InputEvent::ButtonC(Activate) => {
                animations.push(On(Led::C, GREEN));
                Actions::new(self.plethora(PlethoraAction::GoToBoard(3)), animations)
            }
            InputEvent::ButtonF(Activate) => {
                animations.push(On(Led::F, VIOLET));
                Actions::new(self.plethora(PlethoraAction::GoToBoard(4)), animations)
            }
            InputEvent::ButtonD(Activate) => Actions::new(
                self.plethora(PlethoraAction::Board(Direction::Up)),
                animations,
            ),
            InputEvent::ButtonE(Activate) => Actions::new(
                self.plethora(PlethoraAction::Board(Direction::Down)),
                animations,
            ),
            InputEvent::ExpessionPedal(val) => {
                animations.clear();
                let v: u8 = val.into();
                let c = colorous::REDS.eval_rational(v as usize, 127);
                let color = RGB8::new(c.r, c.g, c.b);
                animations.push(On(Led::Clip, color));
                Actions::new(self.audio(PAAction::OutputLevel(val)), animations)
            }
            _ => Actions::default(),
        }
    }
    fn map_live_looper(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(Activate) => {
                Actions::new(self.rc500(RC500Action::ToggleRhythm()), Animations::none())
            }
            InputEvent::ButtonB(Activate) => Actions::new(
                self.rc500(RC500Action::RhythmVariation()),
                Animations::none(),
            ),
            InputEvent::ButtonD(Activate) => Actions::new(
                self.rc500(RC500Action::Mem(Direction::Up)),
                Animations::none(),
            ),
            InputEvent::ButtonE(Activate) => Actions::new(
                self.rc500(RC500Action::Mem(Direction::Down)),
                Animations::none(),
            ),
            InputEvent::ButtonF(Activate) => {
                Actions::new(self.rc500(RC500Action::ClearCurrent()), Animations::none())
            }
            InputEvent::ExpessionPedal(val) => Actions::new(
                self.rc500(RC500Action::CurrentChannelLevel(val)),
                Animations::none(),
            ),
            _ => Actions::default(),
        }
    }
    fn map_setup_looper(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(Activate) => Actions::new(
                self.rc500(RC500Action::RhythmPattern(Direction::Up)),
                Animations::none(),
            ),
            InputEvent::ButtonB(Activate) => Actions::new(
                self.rc500(RC500Action::RhythmPattern(Direction::Down)),
                Animations::none(),
            ),
            InputEvent::ButtonD(Activate) => Actions::new(
                self.rc500(RC500Action::DrumKit(Direction::Up)),
                Animations::none(),
            ),
            InputEvent::ButtonE(Activate) => Actions::new(
                self.rc500(RC500Action::DrumKit(Direction::Down)),
                Animations::none(),
            ),
            _ => Actions::default(),
        }
    }

    fn plethora(&mut self, event: PlethoraAction) -> MidiMessages {
        self.plethora.midi_messages(event)
    }

    fn rc500(&mut self, event: RC500Action) -> MidiMessages {
        self.rc500.midi_messages(event)
    }
    fn audio(&mut self, act: PAAction) -> MidiMessages {
        self.audio.midi_messages(act)
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
        Actions::new(MidiMessages::none(), Animations::none())
    }
}

mod pedalboardaudio;
mod plethora;
mod rc500;

use crate::hmi::inputs::{Edge::Activate, InputEvent};
use defmt::error;
use heapless::Vec;
use midi_types::{MidiMessage, Note};
use smart_leds::{
    colors::{BLUE, DARK_GREEN, GREEN, RED, SEA_GREEN, VIOLET, WHITE, YELLOW},
    RGB8,
};

use crate::hmi::leds::{
    Animation::{Flash, Off, On, Rainbow, Toggle},
    Led, Leds,
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
    leds: [Leds; 3],
    current_mode: usize,
}

pub enum Mode {
    LiveEffect,
    LiveLooper,
    SetupLooper,
}

impl Devices {
    pub fn new() -> Self {
        let leds = Leds::default();
        let mut d = Devices {
            modes: [Mode::LiveEffect, Mode::LiveLooper, Mode::SetupLooper],
            current_mode: 0,
            rc500: RC500::default(),
            audio: PedalboardAudio::default(),
            leds: [leds, Leds::default(), Leds::default()],
            plethora: Plethora {},
        };
        d.change_mode();
        d
    }

    pub fn process_human_input(&mut self, event: InputEvent) -> Actions {
        let actions = match event {
            InputEvent::VolButton(Activate) => {
                self.current_mode += 1;
                if self.current_mode == self.modes.len() {
                    self.current_mode = 0
                }
                self.change_mode();
                Actions::none()
            }
            _ => match self.current_mode() {
                Mode::LiveEffect => self.map_live_effect(event),
                Mode::LiveLooper => self.map_live_looper(event),
                Mode::SetupLooper => self.map_setup_looper(event),
            },
        };
        if !actions.midi_messages.is_empty() {
            self.leds().set(Flash(DARK_GREEN), Led::Mon);
        }

        actions
    }
    pub fn process_midi_input(&mut self, m: MidiMessage) {
        // see https://github.com/pedalboard/db-meter.lv2
        if let MidiMessage::NoteOff(_, Note::C1, vel) = m {
            let v: u8 = vel.into();
            let c = colorous::REDS.eval_rational(v as usize, 127);
            let color = RGB8::new(c.r, c.g, c.b);
            self.leds().set(On(color), Led::L48V);
        }
    }

    pub fn leds(&mut self) -> &mut Leds {
        &mut self.leds[self.current_mode]
    }

    fn current_mode(&mut self) -> &Mode {
        &self.modes[self.current_mode]
    }

    fn map_live_effect(&mut self, event: InputEvent) -> Actions {
        let leds = self.leds();
        match event {
            InputEvent::ButtonA(Activate) => {
                leds.set(On(BLUE), Led::A);
                leds.set(Off, Led::B);
                leds.set(Off, Led::C);
                leds.set(Off, Led::F);
                Actions::new(self.plethora(PlethoraAction::GoToBoard(1)))
            }
            InputEvent::ButtonB(Activate) => {
                leds.set(Off, Led::A);
                leds.set(On(SEA_GREEN), Led::B);
                leds.set(Off, Led::C);
                leds.set(Off, Led::F);
                Actions::new(self.plethora(PlethoraAction::GoToBoard(2)))
            }
            InputEvent::ButtonC(Activate) => {
                leds.set(Off, Led::A);
                leds.set(Off, Led::B);
                leds.set(On(GREEN), Led::C);
                leds.set(Off, Led::F);
                Actions::new(self.plethora(PlethoraAction::GoToBoard(3)))
            }
            InputEvent::ButtonF(Activate) => {
                leds.set(Off, Led::A);
                leds.set(Off, Led::B);
                leds.set(Off, Led::C);
                leds.set(On(VIOLET), Led::F);
                Actions::new(self.plethora(PlethoraAction::GoToBoard(4)))
            }
            InputEvent::ButtonD(Activate) => {
                Actions::new(self.plethora(PlethoraAction::Board(Direction::Up)))
            }
            InputEvent::ButtonE(Activate) => {
                Actions::new(self.plethora(PlethoraAction::Board(Direction::Down)))
            }
            InputEvent::ExpessionPedal(val) => {
                let v: u8 = val.into();
                let c = colorous::REDS.eval_rational(v as usize, 127);
                let color = RGB8::new(c.r, c.g, c.b);
                leds.set(On(color), Led::Clip);
                Actions::new(self.audio(PAAction::OutputLevel(val)))
            }
            _ => Actions::none(),
        }
    }

    fn map_live_looper(&mut self, event: InputEvent) -> Actions {
        let leds = self.leds();
        match event {
            InputEvent::ButtonA(Activate) => {
                leds.set(Toggle(BLUE, true), Led::A);
                Actions::new(self.rc500(RC500Action::ToggleRhythm()))
            }
            InputEvent::ButtonB(Activate) => {
                leds.set(Toggle(BLUE, true), Led::B);
                Actions::new(self.rc500(RC500Action::RhythmVariation()))
            }
            InputEvent::ButtonD(Activate) => {
                Actions::new(self.rc500(RC500Action::Mem(Direction::Up)))
            }
            InputEvent::ButtonE(Activate) => {
                Actions::new(self.rc500(RC500Action::Mem(Direction::Down)))
            }
            InputEvent::ButtonF(Activate) => {
                leds.set(Off, Led::A);
                leds.set(Off, Led::B);
                Actions::new(self.rc500(RC500Action::ClearCurrent()))
            }
            InputEvent::ExpessionPedal(val) => {
                Actions::new(self.rc500(RC500Action::CurrentChannelLevel(val)))
            }
            _ => Actions::none(),
        }
    }
    fn map_setup_looper(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(Activate) => {
                Actions::new(self.rc500(RC500Action::RhythmPattern(Direction::Up)))
            }
            InputEvent::ButtonB(Activate) => {
                Actions::new(self.rc500(RC500Action::RhythmPattern(Direction::Down)))
            }
            InputEvent::ButtonD(Activate) => {
                Actions::new(self.rc500(RC500Action::DrumKit(Direction::Up)))
            }
            InputEvent::ButtonE(Activate) => {
                Actions::new(self.rc500(RC500Action::DrumKit(Direction::Down)))
            }
            _ => Actions::none(),
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

    fn change_mode(&mut self) {
        let mode_color = match self.current_mode() {
            Mode::LiveEffect => {
                self.leds().set(Rainbow(colorous::REDS), Led::D);
                self.leds().set(Rainbow(colorous::BLUES), Led::E);
                WHITE
            }
            Mode::LiveLooper => {
                self.leds().set(Rainbow(colorous::REDS), Led::D);
                self.leds().set(Rainbow(colorous::BLUES), Led::E);
                self.leds().set(On(RED), Led::F);
                RED
            }
            Mode::SetupLooper => {
                self.leds().set(Rainbow(colorous::REDS), Led::D);
                self.leds().set(Rainbow(colorous::BLUES), Led::E);
                self.leds().set(Rainbow(colorous::REDS), Led::A);
                self.leds().set(Rainbow(colorous::BLUES), Led::B);
                YELLOW
            }
        };
        self.leds().set(On(mode_color), Led::Mode);
    }
}

impl Default for Devices {
    fn default() -> Self {
        Self::new()
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

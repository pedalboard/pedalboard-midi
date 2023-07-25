mod pedalboardaudio;
mod plethora;
mod rc500;

use crate::hmi::inputs::{Edge::Activate, InputEvent};
use defmt::*;
use heapless::Vec;
use midi_types::{MidiMessage, Note};
use smart_leds::{colors::*, RGB8};

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
    LiveEffectHandler(LiveEffectHandler),
    LiveLooperHandler(LiveLooperHandler),
    SetupLooperHandler(SetupLooperHandler),
}

impl Handler for HandlerEnum {
    fn handle_human_input(&mut self, e: InputEvent) -> Actions {
        match self {
            HandlerEnum::LiveEffectHandler(h) => h.handle_human_input(e),
            HandlerEnum::LiveLooperHandler(h) => h.handle_human_input(e),
            HandlerEnum::SetupLooperHandler(h) => h.handle_human_input(e),
        }
    }
    fn leds(&mut self) -> &mut Leds {
        match self {
            HandlerEnum::LiveEffectHandler(h) => h.leds(),
            HandlerEnum::LiveLooperHandler(h) => h.leds(),
            HandlerEnum::SetupLooperHandler(h) => h.leds(),
        }
    }
}

pub struct Devices {
    handlers: [HandlerEnum; 3],
    current_mode: usize,
}

impl Devices {
    pub fn new() -> Self {
        Devices {
            handlers: [
                HandlerEnum::LiveEffectHandler(LiveEffectHandler::new()),
                HandlerEnum::LiveLooperHandler(LiveLooperHandler::new()),
                HandlerEnum::SetupLooperHandler(SetupLooperHandler::new()),
            ],
            current_mode: 0,
        }
    }

    pub fn handle_human_input(&mut self, event: InputEvent) -> Actions {
        let actions = match event {
            InputEvent::VolButton(Activate) => {
                self.current_mode += 1;
                if self.current_mode == self.handlers.len() {
                    self.current_mode = 0
                }
                Actions::none()
            }
            _ => self.current_mode().handle_human_input(event),
        };
        if !actions.midi_messages.is_empty() {
            // MIDI-out indicator
            self.current_mode().leds().set(Flash(DARK_GREEN), Led::Mon);
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
                self.current_mode()
                    .leds()
                    .set_ledring(super::hmi::ledring::Animation::Loudness(lufs));
                self.current_mode().leds().set(On(color), Led::L48V);
            }
            _ => {
                // MIDI-in indicator
                self.current_mode().leds().set(Flash(DARK_BLUE), Led::Mon);
            }
        }
    }

    fn current_mode(&mut self) -> &mut HandlerEnum {
        &mut self.handlers[self.current_mode]
    }
    pub fn leds(&mut self) -> &mut Leds {
        self.current_mode().leds()
    }
}

impl Default for Devices {
    fn default() -> Self {
        Self::new()
    }
}

struct LiveEffectHandler {
    leds: Leds,
    plethora: Plethora,
    audio: PedalboardAudio,
}

impl LiveEffectHandler {
    fn new() -> Self {
        let mut leds = Leds::default();
        leds.set(On(RED), Led::D);
        leds.set(On(WHITE), Led::Mode);
        LiveEffectHandler {
            leds,
            plethora: Plethora {},
            audio: PedalboardAudio::default(),
        }
    }
}

impl Handler for LiveEffectHandler {
    fn handle_human_input(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(Activate) => {
                self.leds.set(On(BLUE), Led::A);
                self.leds.set(Off, Led::B);
                self.leds.set(Off, Led::C);
                self.leds.set(Off, Led::F);
                Actions::new(self.plethora.midi_messages(PlethoraAction::GoToBoard(1)))
            }
            InputEvent::ButtonB(Activate) => {
                self.leds.set(Off, Led::A);
                self.leds.set(On(SEA_GREEN), Led::B);
                self.leds.set(Off, Led::C);
                self.leds.set(Off, Led::F);
                Actions::new(self.plethora.midi_messages(PlethoraAction::GoToBoard(2)))
            }
            InputEvent::ButtonC(Activate) => {
                self.leds.set(Off, Led::A);
                self.leds.set(Off, Led::B);
                self.leds.set(On(GREEN), Led::C);
                self.leds.set(Off, Led::F);
                Actions::new(self.plethora.midi_messages(PlethoraAction::GoToBoard(3)))
            }
            InputEvent::ButtonF(Activate) => {
                self.leds.set(Off, Led::A);
                self.leds.set(Off, Led::B);
                self.leds.set(Off, Led::C);
                self.leds.set(On(VIOLET), Led::F);
                Actions::new(self.plethora.midi_messages(PlethoraAction::GoToBoard(4)))
            }
            InputEvent::ButtonD(Activate) => {
                self.leds.set(Toggle(RED, false), Led::D);
                Actions::new(self.audio.midi_messages(PAAction::BypassProcessor(1)))
            }
            InputEvent::ExpessionPedal(val) => {
                let v: u8 = val.into();
                let c = colorous::REDS.eval_rational(v as usize, 127);
                let color = RGB8::new(c.r, c.g, c.b);
                self.leds.set(On(color), Led::Clip);
                Actions::new(self.audio.midi_messages(PAAction::OutputLevel(val)))
            }
            _ => Actions::none(),
        }
    }
    fn leds(&mut self) -> &mut Leds {
        &mut self.leds
    }
}

struct LiveLooperHandler {
    leds: Leds,
    rc500: RC500,
}

impl LiveLooperHandler {
    fn new() -> Self {
        let mut leds = Leds::default();
        leds.set(Rainbow(colorous::REDS), Led::D);
        leds.set(Rainbow(colorous::BLUES), Led::E);
        leds.set(On(RED), Led::F);
        leds.set(On(RED), Led::Mode);

        LiveLooperHandler {
            leds,
            rc500: RC500::default(),
        }
    }
    fn leds(&mut self) -> &mut Leds {
        &mut self.leds
    }
}

impl Handler for LiveLooperHandler {
    fn handle_human_input(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(Activate) => {
                self.leds.set(Toggle(BLUE, true), Led::A);
                Actions::new(self.rc500.midi_messages(RC500Action::ToggleRhythm()))
            }
            InputEvent::ButtonB(Activate) => {
                self.leds.set(Toggle(BLUE, true), Led::B);
                Actions::new(self.rc500.midi_messages(RC500Action::RhythmVariation()))
            }
            InputEvent::ButtonD(Activate) => {
                Actions::new(self.rc500.midi_messages(RC500Action::Mem(Direction::Up)))
            }
            InputEvent::ButtonE(Activate) => {
                Actions::new(self.rc500.midi_messages(RC500Action::Mem(Direction::Down)))
            }
            InputEvent::ButtonF(Activate) => {
                self.leds.set(Off, Led::A);
                self.leds.set(Off, Led::B);
                Actions::new(self.rc500.midi_messages(RC500Action::ClearCurrent()))
            }
            InputEvent::ExpessionPedal(val) => Actions::new(
                self.rc500
                    .midi_messages(RC500Action::CurrentChannelLevel(val)),
            ),
            _ => Actions::none(),
        }
    }
    fn leds(&mut self) -> &mut Leds {
        &mut self.leds
    }
}

struct SetupLooperHandler {
    leds: Leds,
    rc500: RC500,
}

impl SetupLooperHandler {
    fn new() -> Self {
        let mut leds = Leds::default();
        leds.set(Rainbow(colorous::REDS), Led::D);
        leds.set(Rainbow(colorous::BLUES), Led::E);
        leds.set(Rainbow(colorous::REDS), Led::A);
        leds.set(Rainbow(colorous::BLUES), Led::B);
        leds.set(On(YELLOW), Led::Mode);

        SetupLooperHandler {
            leds,
            rc500: RC500::default(),
        }
    }
}

impl Handler for SetupLooperHandler {
    fn handle_human_input(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(Activate) => Actions::new(
                self.rc500
                    .midi_messages(RC500Action::RhythmPattern(Direction::Up)),
            ),
            InputEvent::ButtonB(Activate) => Actions::new(
                self.rc500
                    .midi_messages(RC500Action::RhythmPattern(Direction::Down)),
            ),
            InputEvent::ButtonD(Activate) => Actions::new(
                self.rc500
                    .midi_messages(RC500Action::DrumKit(Direction::Up)),
            ),
            InputEvent::ButtonE(Activate) => Actions::new(
                self.rc500
                    .midi_messages(RC500Action::DrumKit(Direction::Down)),
            ),
            _ => Actions::none(),
        }
    }
    fn leds(&mut self) -> &mut Leds {
        &mut self.leds
    }
}

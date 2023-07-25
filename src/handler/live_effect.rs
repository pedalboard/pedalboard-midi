use crate::devices::pedalboard_audio::{PAAction, PedalboardAudio};
use crate::devices::plethora::{Plethora, PlethoraAction};
use crate::handler::{Actions, Handler};
use crate::hmi::inputs::{Edge::Activate, InputEvent};
use crate::hmi::leds::{
    Animation::{Off, On, Toggle},
    Led, Leds,
};

use smart_leds::{colors::*, RGB8};

pub struct LiveEffect {
    leds: Leds,
    plethora: Plethora,
    audio: PedalboardAudio,
}

impl LiveEffect {
    pub fn new() -> Self {
        let mut leds = Leds::default();
        leds.set(On(RED), Led::D);
        leds.set(On(WHITE), Led::Mode);
        LiveEffect {
            leds,
            plethora: Plethora {},
            audio: PedalboardAudio::default(),
        }
    }
}

impl Handler for LiveEffect {
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
            InputEvent::ExpressionPedal(val) => {
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

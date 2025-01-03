use crate::devices::pedalboard_audio::{PAAction, PedalboardAudio};
use crate::devices::plethora::{Plethora, PlethoraAction};
use crate::handler::{Actions, Handler};
use crate::hmi::inputs::{Edge::Activate, InputEvent};
use crate::hmi::ledring;
use crate::hmi::leds::{Animation::On, Led, LedRings, Leds};
use heapless::Vec;

use smart_leds::{colors::*, RGB8};

pub struct LiveEffect {
    leds: Leds,
    plethora: Plethora,
    audio: PedalboardAudio,
}

impl LiveEffect {
    pub fn new() -> Self {
        let mut leds = Leds::default();
        leds.set_ledring(ledring::Animation::On(RED), LedRings::D);
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
                self.leds
                    .set_ledring(ledring::Animation::On(BLUE), LedRings::A);
                self.leds.set_ledring(ledring::Animation::Off, LedRings::B);
                self.leds.set_ledring(ledring::Animation::Off, LedRings::C);
                self.leds.set_ledring(ledring::Animation::Off, LedRings::F);
                Actions::new(self.plethora.midi_messages(PlethoraAction::GoToBoard(1)))
            }
            InputEvent::ButtonB(Activate) => {
                self.leds.set_ledring(ledring::Animation::Off, LedRings::A);
                self.leds
                    .set_ledring(ledring::Animation::On(SEA_GREEN), LedRings::B);
                self.leds.set_ledring(ledring::Animation::Off, LedRings::C);
                self.leds.set_ledring(ledring::Animation::Off, LedRings::F);
                Actions::new(self.plethora.midi_messages(PlethoraAction::GoToBoard(2)))
            }
            InputEvent::ButtonC(Activate) => {
                self.leds.set_ledring(ledring::Animation::Off, LedRings::A);
                self.leds.set_ledring(ledring::Animation::Off, LedRings::B);
                self.leds
                    .set_ledring(ledring::Animation::On(GREEN), LedRings::C);
                self.leds.set_ledring(ledring::Animation::Off, LedRings::F);
                Actions::new(self.plethora.midi_messages(PlethoraAction::GoToBoard(3)))
            }
            InputEvent::ButtonF(Activate) => {
                self.leds.set_ledring(ledring::Animation::Off, LedRings::A);
                self.leds.set_ledring(ledring::Animation::Off, LedRings::B);
                self.leds.set_ledring(ledring::Animation::Off, LedRings::C);
                self.leds
                    .set_ledring(ledring::Animation::On(VIOLET), LedRings::F);
                Actions::new(self.plethora.midi_messages(PlethoraAction::GoToBoard(4)))
            }
            InputEvent::ButtonD(Activate) => {
                self.leds
                    .set_ledring(ledring::Animation::Toggle(RED, false), LedRings::D);
                Actions::new(self.audio.midi_messages(PAAction::BypassProcessor(1)))
            }
            InputEvent::ExpressionPedalB(val) => {
                let v: u8 = val.into();
                let c = colorous::REDS.eval_rational(v as usize, 127);
                let color = RGB8::new(c.r, c.g, c.b);
                // FIXME improve level display
                self.leds
                    .set_ledring(ledring::Animation::On(color), LedRings::Gain);
                Actions::new(self.audio.midi_messages(PAAction::OutputLevel(val)))
            }
            _ => Actions::none(),
        }
    }
    fn leds(&mut self) -> &mut Leds {
        &mut self.leds
    }
    fn process_sysex(&mut self, _: &[u8]) -> opendeck::config::Responses {
        Vec::new()
    }
}

impl Default for LiveEffect {
    fn default() -> Self {
        Self::new()
    }
}

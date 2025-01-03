use crate::devices::rc500::{RC500Action, RC500};
use crate::devices::Direction;
use crate::handler::{Actions, Handler};
use crate::hmi::inputs::{Edge::Activate, InputEvent};
use crate::hmi::ledring;
use crate::hmi::leds::{Animation::On, Led, LedRings, Leds};
use heapless::Vec;

use smart_leds::colors::*;

pub struct LiveLooper {
    leds: Leds,
    rc500: RC500,
}

impl LiveLooper {
    pub fn new() -> Self {
        let mut leds = Leds::default();
        // FIXME better +/- animation
        leds.set_ledring(ledring::Animation::On(RED), LedRings::D);
        leds.set_ledring(ledring::Animation::On(BLUE), LedRings::E);
        leds.set_ledring(ledring::Animation::On(RED), LedRings::F);
        leds.set(On(RED), Led::Mode);

        LiveLooper {
            leds,
            rc500: RC500::default(),
        }
    }
}

impl Handler for LiveLooper {
    fn handle_human_input(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(Activate) => {
                self.leds
                    .set_ledring(ledring::Animation::Toggle(BLUE, false), LedRings::A);
                Actions::new(self.rc500.midi_messages(RC500Action::ToggleRhythm()))
            }
            InputEvent::ButtonB(Activate) => {
                self.leds
                    .set_ledring(ledring::Animation::Toggle(BLUE, false), LedRings::B);
                Actions::new(self.rc500.midi_messages(RC500Action::RhythmVariation()))
            }
            InputEvent::ButtonD(Activate) => {
                Actions::new(self.rc500.midi_messages(RC500Action::Mem(Direction::Up)))
            }
            InputEvent::ButtonE(Activate) => {
                Actions::new(self.rc500.midi_messages(RC500Action::Mem(Direction::Down)))
            }
            InputEvent::ButtonF(Activate) => {
                self.leds.set_ledring(ledring::Animation::Off, LedRings::A);
                self.leds.set_ledring(ledring::Animation::Off, LedRings::B);
                Actions::new(self.rc500.midi_messages(RC500Action::ClearCurrent()))
            }
            InputEvent::ExpressionPedalB(val) => Actions::new(
                self.rc500
                    .midi_messages(RC500Action::CurrentChannelLevel(val)),
            ),
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

impl Default for LiveLooper {
    fn default() -> Self {
        Self::new()
    }
}

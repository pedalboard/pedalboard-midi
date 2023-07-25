use crate::devices::rc500::{RC500Action, RC500};
use crate::devices::Direction;
use crate::handler::{Actions, Handler};
use crate::hmi::inputs::{Edge::Activate, InputEvent};
use crate::hmi::leds::{
    Animation::{Off, On, Rainbow, Toggle},
    Led, Leds,
};

use smart_leds::colors::*;

pub struct LiveLooperHandler {
    leds: Leds,
    rc500: RC500,
}

impl LiveLooperHandler {
    pub fn new() -> Self {
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

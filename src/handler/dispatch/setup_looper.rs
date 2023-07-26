use crate::devices::rc500::{RC500Action, RC500};
use crate::devices::Direction;
use crate::handler::{Actions, Handler};
use crate::hmi::inputs::{Edge::Activate, InputEvent};
use crate::hmi::leds::{
    Animation::{On, Rainbow},
    Led, Leds,
};

use smart_leds::colors::*;

pub struct SetupLooper {
    leds: Leds,
    rc500: RC500,
}

impl SetupLooper {
    pub fn new() -> Self {
        let mut leds = Leds::default();
        leds.set(Rainbow(colorous::REDS), Led::D);
        leds.set(Rainbow(colorous::BLUES), Led::E);
        leds.set(Rainbow(colorous::REDS), Led::A);
        leds.set(Rainbow(colorous::BLUES), Led::B);
        leds.set(On(YELLOW), Led::Mode);

        SetupLooper {
            leds,
            rc500: RC500::default(),
        }
    }
}

impl Handler for SetupLooper {
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

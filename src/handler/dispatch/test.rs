use crate::handler::{Actions, Handler};
use crate::hmi::inputs::{Edge::Activate, InputEvent};
use crate::hmi::ledring;
use crate::hmi::leds::{Animation::On, Led, LedRings, Leds};

use smart_leds::colors::*;

pub struct Test {
    leds: Leds,
}

impl Test {
    pub fn new() -> Self {
        let mut leds = Leds::default();
        leds.set(On(BLUE), Led::Mode);
        Test { leds }
    }
}

impl Handler for Test {
    fn handle_human_input(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(Activate) => {
                self.leds.set_ledring(ledring::Animation::Off, LedRings::A);
                Actions::none()
            }
            InputEvent::ButtonB(Activate) => {
                self.leds.set_ledring(ledring::Animation::Off, LedRings::B);
                Actions::none()
            }
            InputEvent::ButtonC(Activate) => {
                self.leds.set_ledring(ledring::Animation::Off, LedRings::C);
                Actions::none()
            }
            InputEvent::ButtonD(Activate) => {
                self.leds.set_ledring(ledring::Animation::Off, LedRings::D);
                Actions::none()
            }
            InputEvent::ButtonE(Activate) => {
                self.leds.set_ledring(ledring::Animation::Off, LedRings::E);
                Actions::none()
            }
            InputEvent::ButtonF(Activate) => {
                self.leds.set_ledring(ledring::Animation::Off, LedRings::F);
                Actions::none()
            }
            InputEvent::Vol(v) => {
                let uv: u8 = v.into();
                self.leds.set_ledring(
                    ledring::Animation::Loudness(-100.0 + (uv as f32) * 6.0),
                    LedRings::D,
                );
                Actions::none()
            }
            InputEvent::Gain(v) => {
                let uv: u8 = v.into();
                self.leds.set_ledring(
                    ledring::Animation::Loudness(-100.0 + (uv as f32) * 6.0),
                    LedRings::F,
                );
                Actions::none()
            }
            InputEvent::GainButton(Activate) => {
                self.leds.set_ledring(ledring::Animation::Off, LedRings::F);
                Actions::none()
            }
            InputEvent::ExpressionPedalA(v) => {
                let uv: u8 = v.into();
                self.leds.set_ledring(
                    ledring::Animation::Loudness(-100.0 + (uv as f32) * 6.0),
                    LedRings::A,
                );
                Actions::none()
            }
            InputEvent::ExpressionPedalB(v) => {
                let uv: u8 = v.into();
                self.leds.set_ledring(
                    ledring::Animation::Loudness(-100.0 + (uv as f32) * 6.0),
                    LedRings::C,
                );
                Actions::none()
            }
            _ => Actions::none(),
        }
    }
    fn leds(&mut self) -> &mut Leds {
        &mut self.leds
    }
}

impl Default for Test {
    fn default() -> Self {
        Self::new()
    }
}

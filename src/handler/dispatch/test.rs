use crate::handler::{Actions, Handler, MidiMessages};
use crate::hmi::inputs::{Edge::Activate, Edge::Deactivate, InputEvent};
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
        leds.set_ledring(ledring::Animation::Loudness(-0.), LedRings::A);
        leds.set_ledring(ledring::Animation::Loudness(-0.), LedRings::B);
        leds.set_ledring(ledring::Animation::Loudness(-0.), LedRings::C);
        leds.set_ledring(ledring::Animation::Loudness(-0.), LedRings::D);
        leds.set_ledring(ledring::Animation::Loudness(-0.), LedRings::E);
        leds.set_ledring(ledring::Animation::Loudness(-0.), LedRings::F);
        leds.set_ledring(ledring::Animation::Loudness(-0.), LedRings::Vol);
        leds.set_ledring(ledring::Animation::Loudness(-0.), LedRings::Gain);
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
            InputEvent::ButtonA(Deactivate) => {
                self.leds
                    .set_ledring(ledring::Animation::Loudness(-0.), LedRings::A);
                Actions::none()
            }
            InputEvent::ButtonB(Deactivate) => {
                self.leds
                    .set_ledring(ledring::Animation::Loudness(-0.), LedRings::B);
                Actions::none()
            }
            InputEvent::ButtonC(Deactivate) => {
                self.leds
                    .set_ledring(ledring::Animation::Loudness(-0.), LedRings::C);
                Actions::none()
            }
            InputEvent::ButtonD(Deactivate) => {
                self.leds
                    .set_ledring(ledring::Animation::Loudness(-0.), LedRings::D);
                Actions::none()
            }
            InputEvent::ButtonE(Deactivate) => {
                self.leds
                    .set_ledring(ledring::Animation::Loudness(-0.), LedRings::E);
                Actions::none()
            }
            InputEvent::ButtonF(Deactivate) => {
                self.leds
                    .set_ledring(ledring::Animation::Loudness(-0.), LedRings::F);
                Actions::none()
            }
            InputEvent::Vol(v) => {
                let uv: u8 = v.into();
                self.leds.set_ledring(
                    ledring::Animation::Loudness(-100.0 + (uv as f32) * 6.0),
                    LedRings::Vol,
                );
                Actions::none()
            }
            InputEvent::Gain(v) => {
                let uv: u8 = v.into();
                self.leds.set_ledring(
                    ledring::Animation::Loudness(-100.0 + (uv as f32) * 6.0),
                    LedRings::Gain,
                );
                Actions::none()
            }
            InputEvent::GainButton(Activate) => {
                // send loudness value
                let mut messages: MidiMessages = MidiMessages::none();
                messages.push(midi_types::MidiMessage::NoteOff(
                    midi_types::Channel::C15,
                    midi_types::Note::C1,
                    midi_types::Value7::new(80u8),
                ));
                Actions::new(messages)
            }
            InputEvent::ExpressionPedalA(v) => {
                let uv: u8 = v.into();
                self.leds.set_ledring(
                    ledring::Animation::Loudness(-100.0 + (uv as f32) * 6.0),
                    LedRings::D,
                );
                Actions::none()
            }
            InputEvent::ExpressionPedalB(v) => {
                let uv: u8 = v.into();
                self.leds.set_ledring(
                    ledring::Animation::Loudness(-100.0 + (uv as f32) * 6.0),
                    LedRings::F,
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

use debouncr::{debounce_stateful_3, DebouncerStateful, Repeat3};
use embedded_hal::digital::v2::InputPin;
use rp_pico::hal::gpio::{pin::bank0::Gpio18, Floating, Input, Pin};
pub enum Direction {
    Up,
    Down,
}

pub enum InputEvent {
    ButtonA,
    ButtonB,
    ButtonC,
    ButtonD,
    ButtonE,
    ButtonF,
    ExpessionPedal(midi_types::Value7),
    VolButton,
    Vol(Direction),
    GainButton,
    Gain(Direction),
}

pub struct Inputs {
    rotary_vol_button_pin: Pin<Gpio18, Input<Floating>>,
    rotary_vol_button_state: DebouncerStateful<u8, Repeat3>,
}

impl Inputs {
    pub fn new(rotary_vol_button_pin: Pin<Gpio18, Input<Floating>>) -> Self {
        Self {
            rotary_vol_button_pin,
            rotary_vol_button_state: debounce_stateful_3(false),
        }
    }

    pub fn update(&mut self) -> Option<InputEvent> {
        let edge = self
            .rotary_vol_button_state
            .update(self.rotary_vol_button_pin.is_high().unwrap());
        if edge.is_some() {
            return Some(InputEvent::GainButton);
        }
        None
    }
}

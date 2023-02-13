use debouncr::{debounce_stateful_5, DebouncerStateful, Repeat5};
use embedded_hal::digital::v2::InputPin;
use rp_pico::hal::gpio::{pin::bank0::Gpio18, Input, Pin, PullUp};
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
    vol_button_pin: Pin<Gpio18, Input<PullUp>>,
    vol_button_state: DebouncerStateful<u8, Repeat5>,
}

impl Inputs {
    pub fn new(vol_button_pin: Pin<Gpio18, Input<PullUp>>) -> Self {
        Self {
            vol_button_pin,
            vol_button_state: debounce_stateful_5(false),
        }
    }

    pub fn update(&mut self) -> Option<InputEvent> {
        let edge = self
            .vol_button_state
            .update(self.vol_button_pin.is_high().unwrap());
        if edge.is_some() {
            return Some(InputEvent::GainButton);
        }
        None
    }
}

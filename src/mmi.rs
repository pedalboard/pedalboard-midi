use debouncr::{debounce_stateful_5, DebouncerStateful, Repeat5};
use embedded_hal::digital::v2::InputPin;
use rotary_encoder_embedded::{standard::StandardMode, Direction, RotaryEncoder};
use rp_pico::hal::gpio::{
    pin::bank0::{Gpio16, Gpio17, Gpio18, Gpio19, Gpio20, Gpio21},
    Input, Pin, PullUp,
};

use midi_types::Value7;

pub enum InputEvent {
    ButtonA,
    ButtonB,
    ButtonC,
    ButtonD,
    ButtonE,
    ButtonF,
    ExpessionPedal(Value7),
    VolButton,
    Vol(Value7),
    GainButton,
    Gain(Value7),
}

pub struct Inputs {
    vol_sw_state: DebouncerStateful<u8, Repeat5>,
    vol_sw_pin: Pin<Gpio18, Input<PullUp>>,
    vol_rotary: RotaryEncoder<StandardMode, Pin<Gpio17, Input<PullUp>>, Pin<Gpio16, Input<PullUp>>>,
    vol_value: u8,
    gain_sw_state: DebouncerStateful<u8, Repeat5>,
    gain_sw_pin: Pin<Gpio21, Input<PullUp>>,
}

impl Inputs {
    pub fn new(
        vol_clk_pin: Pin<Gpio16, Input<PullUp>>,
        vol_dt_pin: Pin<Gpio17, Input<PullUp>>,
        vol_sw_pin: Pin<Gpio18, Input<PullUp>>,
        gain_clk_pin: Pin<Gpio19, Input<PullUp>>,
        gain_dt_pin: Pin<Gpio20, Input<PullUp>>,
        gain_sw_pin: Pin<Gpio21, Input<PullUp>>,
    ) -> Self {
        Self {
            vol_sw_pin,
            vol_sw_state: debounce_stateful_5(false),
            gain_sw_pin,
            gain_sw_state: debounce_stateful_5(false),
            vol_rotary: RotaryEncoder::new(vol_dt_pin, vol_clk_pin).into_standard_mode(),
            vol_value: 0,
        }
    }

    pub fn update(&mut self) -> Option<InputEvent> {
        let vol_edge = self.vol_sw_state.update(self.vol_sw_pin.is_high().unwrap());
        if vol_edge.is_some() {
            return Some(InputEvent::VolButton);
        }
        let gain_edge = self
            .gain_sw_state
            .update(self.gain_sw_pin.is_high().unwrap());
        if gain_edge.is_some() {
            return Some(InputEvent::GainButton);
        }
        self.vol_rotary.update();
        match self.vol_rotary.direction() {
            Direction::Clockwise => {
                if self.vol_value < 127 {
                    self.vol_value = self.vol_value + 1;
                }
                Some(InputEvent::Vol(Value7::new(self.vol_value)));
            }
            Direction::Anticlockwise => {
                if self.vol_value > 1 {
                    self.vol_value = self.vol_value - 1;
                }
                Some(InputEvent::Vol(Value7::new(self.vol_value)));
            }
            Direction::None => (),
        }

        None
    }
}

use debouncr::{
    debounce_stateful_5, DebouncerStateful,
    Edge::{Falling, Rising},
    Repeat5,
};
use embedded_hal::digital::v2::InputPin;
use rotary_encoder_embedded::{standard::StandardMode, Direction, RotaryEncoder};
use rp_pico::hal::gpio::{
    pin::bank0::{
        Gpio16, Gpio17, Gpio18, Gpio19, Gpio2, Gpio20, Gpio21, Gpio3, Gpio4, Gpio5, Gpio6, Gpio7,
    },
    Input, Pin, PullUp,
};

use midi_types::Value7;

pub enum Edge {
    Activate,
    Deactivate,
}

pub enum InputEvent {
    ButtonA(Edge),
    ButtonB(Edge),
    ButtonC(Edge),
    ButtonD(Edge),
    ButtonE(Edge),
    ButtonF(Edge),
    ExpessionPedal(Value7),
    VolButton(Edge),
    Vol(Value7),
    GainButton(Edge),
    Gain(Value7),
}

struct Button<PIN> {
    pin: PIN,
    debouncer: DebouncerStateful<u8, Repeat5>,
}

impl<PIN> Button<PIN>
where
    PIN: InputPin,
{
    fn new(pin: PIN) -> Self {
        Button {
            pin,
            debouncer: debounce_stateful_5(false),
        }
    }

    fn update(&mut self) -> Option<Edge> {
        let pressed = self.pin.is_low().unwrap_or_default();
        let edge = self.debouncer.update(pressed);
        edge.map(|e| match e {
            Falling => Edge::Deactivate,
            Rising => Edge::Activate,
        })
    }
}

pub struct Inputs {
    vol_sw: Button<Pin<Gpio18, Input<PullUp>>>,
    vol_rotary: RotaryEncoder<StandardMode, Pin<Gpio17, Input<PullUp>>, Pin<Gpio16, Input<PullUp>>>,
    vol_value: u8,
    gain_sw: Button<Pin<Gpio21, Input<PullUp>>>,
    gain_rotary:
        RotaryEncoder<StandardMode, Pin<Gpio20, Input<PullUp>>, Pin<Gpio19, Input<PullUp>>>,
    gain_value: u8,
    button_a: Button<Pin<Gpio2, Input<PullUp>>>,
    button_b: Button<Pin<Gpio3, Input<PullUp>>>,
    button_c: Button<Pin<Gpio4, Input<PullUp>>>,
    button_d: Button<Pin<Gpio5, Input<PullUp>>>,
    button_e: Button<Pin<Gpio6, Input<PullUp>>>,
    button_f: Button<Pin<Gpio7, Input<PullUp>>>,
}

impl Inputs {
    pub fn new(
        vol_clk_pin: Pin<Gpio16, Input<PullUp>>,
        vol_dt_pin: Pin<Gpio17, Input<PullUp>>,
        vol_sw_pin: Pin<Gpio18, Input<PullUp>>,
        gain_clk_pin: Pin<Gpio19, Input<PullUp>>,
        gain_dt_pin: Pin<Gpio20, Input<PullUp>>,
        gain_sw_pin: Pin<Gpio21, Input<PullUp>>,
        button_a_pin: Pin<Gpio2, Input<PullUp>>,
        button_b_pin: Pin<Gpio3, Input<PullUp>>,
        button_c_pin: Pin<Gpio4, Input<PullUp>>,
        button_d_pin: Pin<Gpio5, Input<PullUp>>,
        button_e_pin: Pin<Gpio6, Input<PullUp>>,
        button_f_pin: Pin<Gpio7, Input<PullUp>>,
    ) -> Self {
        Self {
            vol_sw: Button::new(vol_sw_pin),
            vol_rotary: RotaryEncoder::new(vol_dt_pin, vol_clk_pin).into_standard_mode(),
            vol_value: 0,

            gain_sw: Button::new(gain_sw_pin),
            gain_rotary: RotaryEncoder::new(gain_dt_pin, gain_clk_pin).into_standard_mode(),
            gain_value: 0,

            button_a: Button::new(button_a_pin),
            button_b: Button::new(button_b_pin),
            button_c: Button::new(button_c_pin),
            button_d: Button::new(button_d_pin),
            button_e: Button::new(button_e_pin),
            button_f: Button::new(button_f_pin),
        }
    }

    pub fn update(&mut self) -> Option<InputEvent> {
        self.vol_rotary.update();
        if self.vol_rotary.direction() != Direction::None {
            return Some(InputEvent::Vol(rotary_value(
                self.vol_value,
                self.vol_rotary.direction(),
            )));
        }
        self.gain_rotary.update();
        if self.gain_rotary.direction() != Direction::None {
            return Some(InputEvent::Vol(rotary_value(
                self.gain_value,
                self.gain_rotary.direction(),
            )));
        }

        self.button_a
            .update()
            .map(InputEvent::ButtonA)
            .or_else(|| self.button_b.update().map(InputEvent::ButtonB))
            .or_else(|| self.button_c.update().map(InputEvent::ButtonC))
            .or_else(|| self.button_d.update().map(InputEvent::ButtonD))
            .or_else(|| self.button_e.update().map(InputEvent::ButtonE))
            .or_else(|| self.button_f.update().map(InputEvent::ButtonF))
            .or_else(|| self.vol_sw.update().map(InputEvent::VolButton))
            .or_else(|| self.gain_sw.update().map(InputEvent::GainButton))
            .or(None)
    }
}

fn rotary_value(current: u8, dir: Direction) -> Value7 {
    return Value7::new(match dir {
        Direction::Clockwise => {
            let mut next = current;
            if current < 127 {
                next = current + 1
            }
            next
        }
        Direction::Anticlockwise => {
            let mut next = current;
            if current > 1 {
                next = current - 1;
            }
            next
        }
        Direction::None => current,
    });
}

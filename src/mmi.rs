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

struct Rotary<DT, CLK> {
    encoder: RotaryEncoder<StandardMode, DT, CLK>,
    value: u8,
}

impl<DT, CLK> Rotary<DT, CLK>
where
    DT: InputPin,
    CLK: InputPin,
{
    fn new(dt: DT, clk: CLK) -> Self {
        Rotary {
            encoder: RotaryEncoder::new(dt, clk).into_standard_mode(),
            value: 0u8,
        }
    }
    fn update(&mut self) -> Option<Value7> {
        self.encoder.update();

        match self.encoder.direction() {
            Direction::Clockwise => {
                if self.value < 127 {
                    self.value += 1
                }
                Some(Value7::new(self.value))
            }
            Direction::Anticlockwise => {
                if self.value > 1 {
                    self.value -= 1;
                }
                Some(Value7::new(self.value))
            }
            Direction::None => None,
        }
    }
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
    button_vol: Button<Pin<Gpio18, Input<PullUp>>>,
    vol_rotary: Rotary<Pin<Gpio17, Input<PullUp>>, Pin<Gpio16, Input<PullUp>>>,
    button_gain: Button<Pin<Gpio21, Input<PullUp>>>,
    gain_rotary: Rotary<Pin<Gpio20, Input<PullUp>>, Pin<Gpio19, Input<PullUp>>>,
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
            button_vol: Button::new(vol_sw_pin),
            vol_rotary: Rotary::new(vol_dt_pin, vol_clk_pin),

            button_gain: Button::new(gain_sw_pin),
            gain_rotary: Rotary::new(gain_dt_pin, gain_clk_pin),

            button_a: Button::new(button_a_pin),
            button_b: Button::new(button_b_pin),
            button_c: Button::new(button_c_pin),
            button_d: Button::new(button_d_pin),
            button_e: Button::new(button_e_pin),
            button_f: Button::new(button_f_pin),
        }
    }

    pub fn update(&mut self) -> Option<InputEvent> {
        self.button_a
            .update()
            .map(InputEvent::ButtonA)
            .or_else(|| self.button_b.update().map(InputEvent::ButtonB))
            .or_else(|| self.button_c.update().map(InputEvent::ButtonC))
            .or_else(|| self.button_d.update().map(InputEvent::ButtonD))
            .or_else(|| self.button_e.update().map(InputEvent::ButtonE))
            .or_else(|| self.button_f.update().map(InputEvent::ButtonF))
            .or_else(|| self.button_vol.update().map(InputEvent::VolButton))
            .or_else(|| self.button_gain.update().map(InputEvent::GainButton))
            .or_else(|| self.vol_rotary.update().map(InputEvent::Vol))
            .or_else(|| self.gain_rotary.update().map(InputEvent::Gain))
            .or(None)
    }
}

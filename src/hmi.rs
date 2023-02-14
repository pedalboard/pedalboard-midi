use debouncr::{
    debounce_stateful_5, DebouncerStateful,
    Edge::{Falling, Rising},
    Repeat5,
};
use embedded_hal::digital::v2::InputPin;
use rotary_encoder_embedded::{standard::StandardMode, Direction, RotaryEncoder};
use rp_pico::hal::gpio::{
    pin::{
        bank0::{
            Gpio16, Gpio17, Gpio18, Gpio19, Gpio2, Gpio20, Gpio21, Gpio3, Gpio4, Gpio5, Gpio6,
            Gpio7,
        },
        PinId,
    },
    Input, Pin, PullUp,
};

type PullUpInputPin<I> = Pin<I, Input<PullUp>>;

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

struct Rotary<DT, CLK>
where
    DT: PinId,
    CLK: PinId,
{
    encoder: RotaryEncoder<StandardMode, PullUpInputPin<DT>, PullUpInputPin<CLK>>,
    value: u8,
}

impl<DT, CLK> Rotary<DT, CLK>
where
    DT: PinId,
    CLK: PinId,
{
    fn new(dt: PullUpInputPin<DT>, clk: PullUpInputPin<CLK>) -> Self {
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

struct Button<I>
where
    I: PinId,
{
    pin: PullUpInputPin<I>,
    debouncer: DebouncerStateful<u8, Repeat5>,
}

impl<I> Button<I>
where
    I: PinId,
{
    fn new(pin: PullUpInputPin<I>) -> Self {
        Button {
            pin,
            debouncer: debounce_stateful_5(false),
        }
    }

    fn update(&mut self) -> Option<Edge> {
        let pressed = self.pin.is_low().unwrap();
        let edge = self.debouncer.update(pressed);
        edge.map(|e| match e {
            Falling => Edge::Deactivate,
            Rising => Edge::Activate,
        })
    }
}

pub struct Inputs {
    button_vol: Button<Gpio18>,
    vol_rotary: Rotary<Gpio17, Gpio16>,
    button_gain: Button<Gpio21>,
    gain_rotary: Rotary<Gpio20, Gpio19>,
    button_a: Button<Gpio2>,
    button_b: Button<Gpio3>,
    button_c: Button<Gpio4>,
    button_d: Button<Gpio5>,
    button_e: Button<Gpio6>,
    button_f: Button<Gpio7>,
}

pub struct RotaryPins<DT, CLK, SW>
where
    DT: PinId,
    CLK: PinId,
    SW: PinId,
{
    pub dt: PullUpInputPin<DT>,
    pub clk: PullUpInputPin<CLK>,
    pub sw: PullUpInputPin<SW>,
}

impl Inputs {
    pub fn new(
        vol_pins: RotaryPins<Gpio17, Gpio16, Gpio18>,
        gain_pins: RotaryPins<Gpio20, Gpio19, Gpio21>,
        button_a_pin: PullUpInputPin<Gpio2>,
        button_b_pin: PullUpInputPin<Gpio3>,
        button_c_pin: PullUpInputPin<Gpio4>,
        button_d_pin: PullUpInputPin<Gpio5>,
        button_e_pin: PullUpInputPin<Gpio6>,
        button_f_pin: PullUpInputPin<Gpio7>,
    ) -> Self {
        Self {
            button_vol: Button::new(vol_pins.sw),
            vol_rotary: Rotary::new(vol_pins.dt, vol_pins.clk),

            button_gain: Button::new(gain_pins.sw),
            gain_rotary: Rotary::new(gain_pins.dt, gain_pins.clk),

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

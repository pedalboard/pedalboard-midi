use debouncr::{
    debounce_stateful_5, DebouncerStateful,
    Edge::{Falling, Rising},
    Repeat5,
};
use embedded_hal::adc::OneShot;
use embedded_hal::digital::v2::InputPin;
use movavg::MovAvg;
use rotary_encoder_embedded::{standard::StandardMode, Direction, RotaryEncoder};
use rp_pico::hal::{
    adc::Adc,
    gpio::{
        pin::{
            bank0::{
                Gpio16, Gpio17, Gpio18, Gpio19, Gpio2, Gpio20, Gpio21, Gpio28, Gpio3, Gpio4, Gpio5,
                Gpio6, Gpio7,
            },
            PinId,
        },
        Floating, Input, Pin, PullUp,
    },
};

type PullUpInputPin<I> = Pin<I, Input<PullUp>>;
type FloatingInputPin<I> = Pin<I, Input<Floating>>;
type Sma = MovAvg<u16, u32, 10>;

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
    ExpressionPedal(Value7),
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

pub struct ExpressionPedal {
    current: u8,
    adc: Adc,
    exp_pin: FloatingInputPin<Gpio28>,
    avg: Sma,
    sample_rate_reduction: u8,
}

impl ExpressionPedal {
    fn new(adc: Adc, exp_pin: FloatingInputPin<Gpio28>) -> Self {
        ExpressionPedal {
            adc,
            exp_pin,
            current: 0,
            avg: MovAvg::new(),
            sample_rate_reduction: 0,
        }
    }

    fn update(&mut self) -> Option<Value7> {
        self.sample_rate_reduction += 1;
        if self.sample_rate_reduction <= 25 {
            return None;
        }
        self.sample_rate_reduction = 0;

        let pin_adc_counts: u16 = self.adc.read(&mut self.exp_pin).unwrap();

        let new = (self.avg.feed(pin_adc_counts) >> 5) as u8;

        if self.current.abs_diff(new) > 2 {
            self.current = new;
            return Some(Value7::new(self.current));
        }

        None
    }
}

#[cfg(feature = "hw-v1")]
pub struct Inputs {
    button_vol: Button<Gpio18>,
    vol_rotary: Rotary<Gpio17, Gpio16>,
    button_gain: Button<Gpio21>,
    gain_rotary: Rotary<Gpio20, Gpio19>,
    button_a: Button<Gpio7>,
    button_b: Button<Gpio5>,
    button_c: Button<Gpio2>,
    button_d: Button<Gpio6>,
    button_e: Button<Gpio4>,
    button_f: Button<Gpio3>,
    exp: ExpressionPedal,
}

#[cfg(not(feature = "hw-v1"))]
pub struct Inputs {
    button_vol: Button<Gpio18>,
    vol_rotary: Rotary<Gpio17, Gpio16>,
    button_gain: Button<Gpio21>,
    gain_rotary: Rotary<Gpio20, Gpio19>,
    button_a: Button<Gpio6>,
    button_b: Button<Gpio5>,
    button_c: Button<Gpio2>,
    button_d: Button<Gpio7>,
    button_e: Button<Gpio4>,
    button_f: Button<Gpio3>,
    exp: ExpressionPedal,
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

pub struct ButtonPins(
    pub PullUpInputPin<Gpio7>,
    pub PullUpInputPin<Gpio5>,
    pub PullUpInputPin<Gpio2>,
    pub PullUpInputPin<Gpio6>,
    pub PullUpInputPin<Gpio4>,
    pub PullUpInputPin<Gpio3>,
);

impl Inputs {
    pub fn new(
        vol_pins: RotaryPins<Gpio17, Gpio16, Gpio18>,
        gain_pins: RotaryPins<Gpio20, Gpio19, Gpio21>,
        button_pins: ButtonPins,
        adc: Adc,
        exp_pin: FloatingInputPin<Gpio28>,
    ) -> Self {
        let (ba, bd) = match () {
            #[cfg(not(feature = "hw-v1"))]
            () => (button_pins.3, button_pins.0),
            #[cfg(feature = "hw-v1")]
            () => (button_pins.0, button_pins.3),
        };
        Self {
            button_vol: Button::new(vol_pins.sw),
            vol_rotary: Rotary::new(vol_pins.dt, vol_pins.clk),

            button_gain: Button::new(gain_pins.sw),
            gain_rotary: Rotary::new(gain_pins.dt, gain_pins.clk),

            button_a: Button::new(ba),
            button_b: Button::new(button_pins.1),
            button_c: Button::new(button_pins.2),
            button_d: Button::new(bd),
            button_e: Button::new(button_pins.4),
            button_f: Button::new(button_pins.5),

            exp: ExpressionPedal::new(adc, exp_pin),
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
            .or_else(|| self.exp.update().map(InputEvent::ExpressionPedal))
            .or(None)
    }
}

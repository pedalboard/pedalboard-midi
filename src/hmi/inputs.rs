use debouncr::{
    debounce_stateful_5, DebouncerStateful,
    Edge::{Falling, Rising},
    Repeat5,
};
use defmt::Format;
use embedded_hal::digital::InputPin;
use movavg::MovAvg;
use rotary_encoder_embedded::{standard::StandardMode, Direction, RotaryEncoder};
use rp2040_hal::adc::AdcFifo;
type Sma = MovAvg<u16, u32, 10>;

use midi_types::Value7;

#[derive(Format)]
pub enum Edge {
    Activate,
    Deactivate,
}
#[derive(Format)]
pub enum InputEvent {
    ButtonA(Edge),
    ButtonB(Edge),
    ButtonC(Edge),
    ButtonD(Edge),
    ButtonE(Edge),
    ButtonF(Edge),
    ExpressionPedalA(Value7),
    ExpressionPedalB(Value7),
    VolButton(Edge),
    Vol(Value7),
    GainButton(Edge),
    Gain(Value7),
}

pub struct Rotary<DT, CLK, B> {
    encoder: RotaryEncoder<StandardMode, DT, CLK>,
    button: Button<B>,
    value: u8,
}

impl<DT, CLK, B> Rotary<DT, CLK, B>
where
    DT: InputPin,
    CLK: InputPin,
    B: InputPin,
{
    pub fn new(pin_dt: DT, pin_clk: CLK, pin_b: B) -> Self {
        Rotary {
            encoder: RotaryEncoder::new(pin_dt, pin_clk).into_standard_mode(),
            button: Button::new(pin_b),
            value: 0u8,
        }
    }
    fn update(&mut self) -> Option<Value7> {
        match self.encoder.update() {
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

pub struct Button<P> {
    pin: P,
    debouncer: DebouncerStateful<u8, Repeat5>,
}

impl<P: InputPin> Button<P> {
    pub fn new(pin: P) -> Self {
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

pub struct ExpressionPedals {
    sample_rate_reduction: u8,
    exp_a: ExpressionPedal,
    exp_b: ExpressionPedal,
    adc_fifo: AdcFifo<'static, u16>,
}

impl ExpressionPedals {
    pub fn new(adc_fifo: AdcFifo<'static, u16>) -> Self {
        ExpressionPedals {
            sample_rate_reduction: 0,
            exp_a: ExpressionPedal::new(),
            exp_b: ExpressionPedal::new(),
            adc_fifo,
        }
    }
    fn update(&mut self) -> (Option<Value7>, Option<Value7>) {
        self.sample_rate_reduction += 1;
        if self.sample_rate_reduction <= 25 {
            return (None, None);
        }
        self.sample_rate_reduction = 0;
        self.adc_fifo.resume();
        while self.adc_fifo.len() < 2 {}
        self.adc_fifo.pause();

        let exp_a: u16 = self.adc_fifo.read(); //self.adc.read(&mut self.exp_a_pin).unwrap();
        let exp_b: u16 = self.adc_fifo.read(); // self.adc.read(&mut self.exp_b_pin).unwrap();
        (self.exp_a.update(exp_a), self.exp_b.update(exp_b))
    }
}

pub struct ExpressionPedal {
    current: u8,
    avg: Sma,
}

impl ExpressionPedal {
    fn new() -> Self {
        ExpressionPedal {
            current: 0,
            avg: MovAvg::new(),
        }
    }

    fn update(&mut self, value: u16) -> Option<Value7> {
        let new = (self.avg.feed(value) >> 5) as u8;

        if self.current.abs_diff(new) > 2 {
            self.current = new;
            return Some(Value7::new(self.current));
        }

        None
    }
}

pub struct Buttons<PBA, PBB, PBC, PBD, PBE, PBF> {
    a: Button<PBA>,
    b: Button<PBB>,
    c: Button<PBC>,
    d: Button<PBD>,
    e: Button<PBE>,
    f: Button<PBF>,
}

impl<PBA, PBB, PBC, PBD, PBE, PBF> Buttons<PBA, PBB, PBC, PBD, PBE, PBF>
where
    PBA: InputPin,
    PBB: InputPin,
    PBC: InputPin,
    PBD: InputPin,
    PBE: InputPin,
    PBF: InputPin,
{
    pub fn new(a: PBA, b: PBB, c: PBC, d: PBD, e: PBE, f: PBF) -> Self {
        Buttons {
            a: Button::new(a),
            b: Button::new(b),
            c: Button::new(c),
            d: Button::new(d),
            e: Button::new(e),
            f: Button::new(f),
        }
    }
}

pub struct Inputs<PBA, PBB, PBC, PBD, PBE, PBF, VDT, VCLK, VB, GDT, GCLK, GB> {
    //    button_vol: Button<Gpio18>,
    vol_rotary: Rotary<VDT, VCLK, VB>,
    //    button_gain: Button<Gpio21>,
    gain_rotary: Rotary<GDT, GCLK, GB>,
    buttons: Buttons<PBA, PBB, PBC, PBD, PBE, PBF>,
    exp: ExpressionPedals,
}

impl<PBA, PBB, PBC, PBD, PBE, PBF, VDT, VCLK, VB, GDT, GCLK, GB>
    Inputs<PBA, PBB, PBC, PBD, PBE, PBF, VDT, VCLK, VB, GDT, GCLK, GB>
where
    PBA: InputPin,
    PBB: InputPin,
    PBC: InputPin,
    PBD: InputPin,
    PBE: InputPin,
    PBF: InputPin,
    VDT: InputPin,
    VCLK: InputPin,
    VB: InputPin,
    GDT: InputPin,
    GCLK: InputPin,
    GB: InputPin,
{
    pub fn new(
        vol_rotary: Rotary<VDT, VCLK, VB>,
        gain_rotary: Rotary<GDT, GCLK, GB>,
        buttons: Buttons<PBA, PBB, PBC, PBD, PBE, PBF>,
        exp: ExpressionPedals,
    ) -> Self {
        Self {
            vol_rotary,
            gain_rotary,
            buttons,
            exp,
        }
    }

    pub fn update(&mut self) -> Option<InputEvent> {
        let (exp_a, exp_b) = self.exp.update();
        self.buttons
            .a
            .update()
            .map(InputEvent::ButtonA)
            .or_else(|| self.buttons.b.update().map(InputEvent::ButtonB))
            .or_else(|| self.buttons.c.update().map(InputEvent::ButtonC))
            .or_else(|| self.buttons.d.update().map(InputEvent::ButtonD))
            .or_else(|| self.buttons.e.update().map(InputEvent::ButtonE))
            .or_else(|| self.buttons.f.update().map(InputEvent::ButtonF))
            .or_else(|| self.vol_rotary.button.update().map(InputEvent::VolButton))
            .or_else(|| self.gain_rotary.button.update().map(InputEvent::GainButton))
            .or_else(|| self.vol_rotary.update().map(InputEvent::Vol))
            .or_else(|| self.gain_rotary.update().map(InputEvent::Gain))
            .or_else(|| exp_a.map(InputEvent::ExpressionPedalA))
            .or_else(|| exp_b.map(InputEvent::ExpressionPedalB))
            .or(None)
    }
}

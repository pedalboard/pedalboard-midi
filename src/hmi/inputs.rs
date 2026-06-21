use debouncr::{
    debounce_stateful_5, DebouncerStateful,
    Edge::{Falling, Rising},
    Repeat5,
};
use embedded_hal::digital::InputPin;
use movavg::MovAvg;
use pedalboard_midi::events::{Edge, InputEvent, Pulse};
use rotary_encoder_embedded::{quadrature::QuadratureTableMode, Direction, RotaryEncoder};
use rp2040_hal::adc::AdcFifo;
type Sma = MovAvg<u16, u32, 10>;

pub struct Rotary<DT, CLK, B> {
    encoder: RotaryEncoder<QuadratureTableMode, DT, CLK>,
    button: Button<B>,
    cooldown: u8,
}

impl<DT, CLK, B> Rotary<DT, CLK, B>
where
    DT: InputPin,
    CLK: InputPin,
    B: InputPin,
{
    pub fn new(pin_dt: DT, pin_clk: CLK, pin_b: B) -> Self {
        Rotary {
            encoder: RotaryEncoder::new(pin_dt, pin_clk).into_quadrature_table_mode(2),
            button: Button::new(pin_b),
            cooldown: 0,
        }
    }
    fn update(&mut self) -> Option<Pulse> {
        let dir = self.encoder.update();
        if self.cooldown > 0 {
            self.cooldown -= 1;
            return None;
        }
        match dir {
            Direction::Clockwise => {
                self.cooldown = 5; // ignore for 5 poll cycles (~5ms)
                Some(Pulse::Clockwise)
            }
            Direction::Anticlockwise => {
                self.cooldown = 5;
                Some(Pulse::CounterClockwise)
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
    fn update(&mut self) -> (Option<u16>, Option<u16>) {
        self.sample_rate_reduction += 1;
        if self.sample_rate_reduction <= 25 {
            return (None, None);
        }
        self.sample_rate_reduction = 0;
        self.adc_fifo.resume();
        while self.adc_fifo.len() < 4 {}
        self.adc_fifo.pause();

        // Discard first sample of each channel (ADC crosstalk settling)
        let _discard_a: u16 = self.adc_fifo.read().unwrap_or(0);
        let _discard_b: u16 = self.adc_fifo.read().unwrap_or(0);
        let exp_a: u16 = self.adc_fifo.read().unwrap_or(0);
        let exp_b: u16 = self.adc_fifo.read().unwrap_or(0);
        (self.exp_a.update(exp_a), self.exp_b.update(exp_b))
    }
}

pub struct ExpressionPedal {
    current: u16,
    avg: Sma,
}

impl ExpressionPedal {
    fn new() -> Self {
        ExpressionPedal {
            current: 0,
            avg: MovAvg::new(),
        }
    }

    fn update(&mut self, value: u16) -> Option<u16> {
        let new = self.avg.feed(value);

        if self.current.abs_diff(new) > 2 {
            self.current = new;
            return Some(self.current);
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

    /// Poll only encoders — call at high frequency to avoid missing transitions.
    pub fn poll_encoders(&mut self, events: &mut heapless::Vec<InputEvent, 14>) {
        if let Some(e) = self.vol_rotary.update().map(InputEvent::Vol) {
            events.push(e).ok();
        }
        if let Some(e) = self.gain_rotary.update().map(InputEvent::Gain) {
            events.push(e).ok();
        }
    }

    pub fn update(&mut self) -> heapless::Vec<InputEvent, 14> {
        let mut events = heapless::Vec::new();
        let (exp_a, exp_b) = self.exp.update();

        if let Some(e) = self.buttons.a.update().map(InputEvent::ButtonA) {
            events.push(e).ok();
        }
        if let Some(e) = self.buttons.b.update().map(InputEvent::ButtonB) {
            events.push(e).ok();
        }
        if let Some(e) = self.buttons.c.update().map(InputEvent::ButtonC) {
            events.push(e).ok();
        }
        if let Some(e) = self.buttons.d.update().map(InputEvent::ButtonD) {
            events.push(e).ok();
        }
        if let Some(e) = self.buttons.e.update().map(InputEvent::ButtonE) {
            events.push(e).ok();
        }
        if let Some(e) = self.buttons.f.update().map(InputEvent::ButtonF) {
            events.push(e).ok();
        }
        if let Some(e) = self.vol_rotary.button.update().map(InputEvent::VolButton) {
            events.push(e).ok();
        }
        if let Some(e) = self.gain_rotary.button.update().map(InputEvent::GainButton) {
            events.push(e).ok();
        }
        if let Some(e) = exp_a.map(InputEvent::ExpressionPedalA) {
            events.push(e).ok();
        }
        if let Some(e) = exp_b.map(InputEvent::ExpressionPedalB) {
            events.push(e).ok();
        }
        events
    }
}

use super::ledring::LedRing;
use crate::hmi::ledring::LEDS_PER_RING;
use colorous::Gradient;
use rp2040_hal::{
    gpio::{
        bank0::{Gpio11, Gpio12, Gpio14},
        FunctionSpi, Pin, PullDown,
    },
    pac::SPI1,
    spi::{Enabled, Spi},
};
use smart_leds::RGB8;
use ws2812_spi::Ws2812;

const NUM_LEDS: usize = 2;
const NUM_LED_RINGS: usize = 8;
const LED_OUTPUTS: usize = NUM_LEDS + NUM_LED_RINGS * LEDS_PER_RING;

pub type LedSpi = Spi<
    Enabled,
    SPI1,
    (
        Pin<Gpio11, FunctionSpi, PullDown>,
        Pin<Gpio12, FunctionSpi, PullDown>,
        Pin<Gpio14, FunctionSpi, PullDown>,
    ),
>;

pub type LedDriver = Ws2812<LedSpi>;

#[derive(Debug, Clone, Copy)]
pub enum Led {
    Mode,
    Mon,
}

#[derive(Debug, Clone, Copy)]
pub enum LedRings {
    Gain,
    F,
    C,
    B,
    E,
    Vol,
    D,
    A,
}

pub type LedData = [RGB8; LED_OUTPUTS];

#[derive(Debug, Clone, Copy)]
pub enum Animation {
    On(RGB8),
    Off,
    Toggle(RGB8, bool),
    Flash(RGB8),
    Rainbow(Gradient),
}

pub struct Leds {
    sawtooth: Sawtooth,
    animations: [Animation; NUM_LEDS],
    ledrings: [LedRing; NUM_LED_RINGS],
}

impl Leds {
    pub fn new() -> Self {
        Leds {
            sawtooth: Sawtooth::new(),
            animations: [Animation::Off; NUM_LEDS],
            ledrings: [
                LedRing::default(),
                LedRing::default(),
                LedRing::default(),
                LedRing::default(),
                LedRing::default(),
                LedRing::default(),
                LedRing::default(),
                LedRing::default(),
            ],
        }
    }
    pub fn animate(&mut self) -> LedData {
        let mut data: LedData = [RGB8::default(); LED_OUTPUTS];
        self.sawtooth.next();

        // process the led ring animations
        for (ring_index, mut ring) in self.ledrings.into_iter().enumerate() {
            for (led_index, ring_led) in ring.animate().into_iter().enumerate() {
                data[ring_index * LEDS_PER_RING + led_index] = ring_led;
            }
        }

        // process the single led animations
        for (single, a) in self.animations.into_iter().enumerate() {
            let led = (NUM_LED_RINGS * LEDS_PER_RING) + single;
            match a {
                Animation::On(c) => data[led] = c,
                Animation::Off => data[led] = RGB8::default(),
                Animation::Toggle(c, true) => data[led] = c,
                Animation::Toggle(_, false) => data[led] = RGB8::default(),
                Animation::Flash(c) => {
                    data[led] = c;
                    self.animations[single] = Animation::Off
                }
                Animation::Rainbow(gradient) => {
                    let c = gradient.eval_rational(self.sawtooth.value, self.sawtooth.max);
                    data[led].r = c.r;
                    data[led].g = c.g;
                    data[led].b = c.b;
                }
            };
        }

        data
    }

    pub fn set(&mut self, a: Animation, l: Led) {
        let index = l as usize;
        match a {
            Animation::Toggle(c, _) => {
                let current_animation = self.animations[index];
                match current_animation {
                    Animation::Toggle(_, true) => {
                        self.animations[index] = Animation::Toggle(c, false)
                    }
                    Animation::Toggle(_, false) => {
                        self.animations[index] = Animation::Toggle(c, true)
                    }
                    _ => self.animations[index] = a,
                };
            }
            _ => self.animations[index] = a,
        }
    }
    pub fn set_ledring(&mut self, a: super::ledring::Animation, r: LedRings) {
        let ri = r as usize;
        self.ledrings[ri].set(a)
    }
}

impl Default for Leds {
    fn default() -> Self {
        Self::new()
    }
}

struct Sawtooth {
    value: usize,
    rising: bool,
    max: usize,
    min: usize,
}

impl Sawtooth {
    fn next(&mut self) -> usize {
        if self.value == self.max {
            self.rising = false;
        }
        if self.value == self.min {
            self.rising = true;
        }
        if self.rising {
            self.value += 1
        } else {
            self.value -= 1
        }
        self.value
    }
    fn new() -> Self {
        Sawtooth {
            value: 8,
            rising: true,
            max: 16,
            min: 8,
        }
    }
}

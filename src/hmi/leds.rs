use super::ledring::LedRing;
use crate::hmi::ledring::LEDS_PER_RING;
use colorous::Gradient;
use smart_leds::RGB8;

const NUM_LEDS: usize = 4;
const NUM_LED_RINGS: usize = 6;
const LED_OUTPUTS: usize = NUM_LEDS + NUM_LED_RINGS * LEDS_PER_RING;

#[derive(Debug, Clone, Copy)]
pub enum Led {
    Mode,
    Mon,
    L48V,
    Clip,
}

#[derive(Debug, Clone, Copy)]
pub enum LedRings {
    F,
    C,
    B,
    E,
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
            ledrings: [LedRing::new(); NUM_LED_RINGS],
        }
    }
    pub fn animate(&mut self) -> LedData {
        let mut data: LedData = [RGB8::default(); LED_OUTPUTS];
        self.sawtooth.next();

        for (led, a) in self.animations.into_iter().enumerate() {
            match a {
                Animation::On(c) => {
                    data[led].r = c.g;
                    data[led].g = c.r;
                    data[led].b = c.b;
                }
                Animation::Off => {
                    data[led].r = 0;
                    data[led].g = 0;
                    data[led].b = 0;
                }
                Animation::Toggle(c, true) => {
                    data[led].r = c.g;
                    data[led].g = c.r;
                    data[led].b = c.b;
                }
                Animation::Toggle(_, false) => {
                    data[led].r = 0;
                    data[led].g = 0;
                    data[led].b = 0;
                }
                Animation::Flash(c) => {
                    data[led].r = c.g;
                    data[led].g = c.r;
                    data[led].b = c.b;
                    self.animations[led] = Animation::Off
                }
                Animation::Rainbow(gradient) => {
                    let c = gradient.eval_rational(self.sawtooth.value, self.sawtooth.max);
                    data[led].r = c.g;
                    data[led].g = c.r;
                    data[led].b = c.b;
                }
            };
        }

        for (ring_index, mut ring) in self.ledrings.into_iter().enumerate() {
            for (led_index, ring_led) in ring.animate().into_iter().enumerate() {
                data[NUM_LEDS + ring_index * LEDS_PER_RING + led_index] = ring_led;
            }
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

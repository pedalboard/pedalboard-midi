use crate::ledring::{LedRing, LEDS_PER_RING};
use colorous::Gradient;
use smart_leds::RGB8;

const NUM_LEDS: usize = 2;
const NUM_LED_RINGS: usize = 8;
pub const LED_OUTPUTS: usize = NUM_LEDS + NUM_LED_RINGS * LEDS_PER_RING;

pub type LedData = [RGB8; LED_OUTPUTS];

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum Led {
    Mode,
    Mon,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
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

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
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
            ledrings: [LedRing::default(); NUM_LED_RINGS],
        }
    }

    pub fn animate(&mut self) -> LedData {
        let mut data: LedData = [RGB8::default(); LED_OUTPUTS];
        self.sawtooth.next();

        for (ring_index, ring) in self.ledrings.iter_mut().enumerate() {
            for (led_index, ring_led) in ring.animate().into_iter().enumerate() {
                data[ring_index * LEDS_PER_RING + led_index] = ring_led;
            }
        }

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

    pub fn set_ledring(&mut self, a: crate::ledring::Animation, r: LedRings) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use smart_leds::colors::*;

    #[test]
    fn test_leds_default_all_off() {
        let mut leds = Leds::new();
        let data = leds.animate();
        for led in data.iter() {
            assert_eq!(*led, RGB8::default());
        }
    }

    #[test]
    fn test_set_led_on() {
        let mut leds = Leds::new();
        leds.set(Animation::On(RED), Led::Mon);
        let data = leds.animate();
        let mon_index = NUM_LED_RINGS * LEDS_PER_RING + Led::Mon as usize;
        assert_eq!(data[mon_index], RED);
    }

    #[test]
    fn test_flash_turns_off_after_one_frame() {
        let mut leds = Leds::new();
        leds.set(Animation::Flash(GREEN), Led::Mode);
        let data = leds.animate();
        let mode_index = NUM_LED_RINGS * LEDS_PER_RING + Led::Mode as usize;
        assert_eq!(data[mode_index], GREEN);

        // Next frame should be off
        let data = leds.animate();
        assert_eq!(data[mode_index], RGB8::default());
    }

    #[test]
    fn test_toggle_flips_state() {
        let mut leds = Leds::new();
        leds.set(Animation::Toggle(BLUE, false), Led::Mon);
        let data = leds.animate();
        let mon_index = NUM_LED_RINGS * LEDS_PER_RING + Led::Mon as usize;
        assert_eq!(data[mon_index], RGB8::default());

        leds.set(Animation::Toggle(BLUE, false), Led::Mon);
        let data = leds.animate();
        assert_eq!(data[mon_index], BLUE);
    }
}

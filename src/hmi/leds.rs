use colorous::Gradient;
use smart_leds::RGB8;

const NUM_LEDS: usize = 10;

#[derive(Debug, Clone, Copy)]
pub enum Led {
    Clip,
    F,
    C,
    B,
    A,
    D,
    Mode,
    Mon,
    E,
    L48V,
}

pub type LedData = [RGB8; NUM_LEDS];

#[derive(Debug, Clone, Copy)]
pub enum Animation {
    On(RGB8),
    Off,
    Flash(RGB8),
    Rainbow(Gradient),
}

pub struct Leds {
    iteration: u8,
    animations: [Animation; NUM_LEDS],
}

impl Leds {
    pub fn new() -> Self {
        Leds {
            iteration: 0,
            animations: [Animation::Off; NUM_LEDS],
        }
    }
    pub fn animate(&mut self) -> LedData {
        self.iteration += 1;

        let mut data: LedData = [RGB8::default(); NUM_LEDS];

        let mut led: usize = 0;
        for a in self.animations {
            match a {
                Animation::On(c) => {
                    data[led].r = c.r;
                    data[led].g = c.g;
                    data[led].b = c.b;
                }
                Animation::Off => {
                    data[led].r = 0;
                    data[led].g = 0;
                    data[led].b = 0;
                }
                Animation::Flash(c) => {
                    data[led].r = c.r;
                    data[led].g = c.g;
                    data[led].b = c.b;
                    self.animations[led] = Animation::Off
                }
                Animation::Rainbow(gradient) => {
                    let c = gradient.eval_rational(
                        ((self.iteration * 4).rem_euclid(core::u8::MAX)) as usize,
                        core::u8::MAX as usize,
                    );
                    data[led].r = c.r;
                    data[led].g = c.g;
                    data[led].b = c.b;
                }
            };
            led += 1
        }
        data
    }

    pub fn set(&mut self, a: Animation, l: Led) {
        self.animations[l as usize] = a
    }
}

impl Default for Leds {
    fn default() -> Self {
        Self::new()
    }
}

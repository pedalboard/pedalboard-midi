use smart_leds::RGB8;
pub const LEDS_PER_RING: usize = 12;

pub type LedData = [RGB8; LEDS_PER_RING];

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum Animation {
    On(RGB8),
    Off,
    Toggle(RGB8, bool),
    Flash(RGB8),
    Loudness(f32),
}

pub struct LedRing {
    animation: Animation,
}

impl LedRing {
    pub fn new() -> Self {
        LedRing {
            animation: Animation::Loudness(-0.),
        }
    }
    pub fn animate(&mut self) -> LedData {
        match self.animation {
            Animation::On(c) => [c; LEDS_PER_RING],
            Animation::Off => [RGB8::default(); LEDS_PER_RING],
            Animation::Toggle(c, true) => [c; LEDS_PER_RING],
            Animation::Toggle(_, false) => [RGB8::default(); LEDS_PER_RING],
            Animation::Flash(c) => {
                self.animation = Animation::Off;
                [c; LEDS_PER_RING]
            }
            Animation::Loudness(lufs) => {
                let mut data = [RGB8::default(); LEDS_PER_RING];
                for i in 0..LEDS_PER_RING {
                    let reference = crate::loudness::loudness_step(i);
                    if lufs >= reference {
                        data[(LEDS_PER_RING - i) % LEDS_PER_RING] =
                            crate::loudness::loudness_color(reference);
                    }
                }
                data
            }
        }
    }

    pub fn set(&mut self, a: Animation) {
        match a {
            Animation::Toggle(c, _) => {
                let current_animation = self.animation;
                match current_animation {
                    Animation::Toggle(_, true) => self.animation = Animation::Toggle(c, false),
                    Animation::Toggle(_, false) => self.animation = Animation::Toggle(c, true),
                    _ => self.animation = a,
                };
            }
            _ => self.animation = a,
        }
    }
}

impl Default for LedRing {
    fn default() -> Self {
        Self::new()
    }
}

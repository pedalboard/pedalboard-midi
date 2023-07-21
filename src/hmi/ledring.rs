use smart_leds::RGB8;

const NUM_LEDS: usize = 12;

pub type LedData = [RGB8; NUM_LEDS];

#[derive(Debug, Clone, Copy)]
pub enum Animation {
    On(RGB8),
    Off,
    Toggle(RGB8, bool),
    Flash(RGB8),
}

pub struct LedRing {
    animation: Animation,
}

impl LedRing {
    pub fn new() -> Self {
        LedRing {
            animation: Animation::Off,
        }
    }
    pub fn animate(&mut self) -> LedData {
        match self.animation {
            Animation::On(c) => [c; NUM_LEDS],
            Animation::Off => [RGB8::default(); NUM_LEDS],
            Animation::Toggle(c, true) => [c; NUM_LEDS],
            Animation::Toggle(_, false) => [RGB8::default(); NUM_LEDS],
            Animation::Flash(c) => {
                self.animation = Animation::Off;
                [c; NUM_LEDS]
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

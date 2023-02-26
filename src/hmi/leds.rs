use defmt::error;
use heapless::Vec;
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

type LedData = [RGB8; NUM_LEDS];

#[derive(Debug, Clone, Copy)]
pub enum Animation {
    On(Led, RGB8),
    Off(Led),
    Flash(Led, RGB8),
}

pub struct Leds {
    data: LedData,
}

type AnimationVec = Vec<Animation, 8>;

#[derive(Debug)]
pub struct Animations(AnimationVec);

impl Animations {
    pub fn none() -> Self {
        Animations(Vec::new())
    }

    pub fn with_only(a: Animation) -> Self {
        let mut animations = Animations::none();
        animations.push(a);
        animations
    }

    pub fn push(&mut self, a: Animation) {
        if self.0.push(a).is_err() {
            error!("failed pushing ainimation")
        };
    }

    pub fn all_button_leds_off(&mut self) {
        self.push(Animation::Off(Led::A));
        self.push(Animation::Off(Led::B));
        self.push(Animation::Off(Led::C));
        self.push(Animation::Off(Led::D));
        self.push(Animation::Off(Led::E));
        self.push(Animation::Off(Led::F));
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn animations(self) -> AnimationVec {
        self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Leds {
    pub fn new() -> Self {
        Leds {
            data: [RGB8::default(); NUM_LEDS],
        }
    }
    pub fn animate(&mut self, a: Animation) -> (LedData, Option<Animation>) {
        let next = match a {
            Animation::On(led, c) => {
                self.data[led as usize].r = c.r;
                self.data[led as usize].g = c.g;
                self.data[led as usize].b = c.b;
                None
            }
            Animation::Off(led) => {
                self.data[led as usize].r = 0;
                self.data[led as usize].g = 0;
                self.data[led as usize].b = 0;
                None
            }
            Animation::Flash(led, c) => {
                self.data[led as usize].r = c.r;
                self.data[led as usize].g = c.g;
                self.data[led as usize].b = c.b;
                Some(Animation::Off(led))
            }
        };
        (self.data, next)
    }
}

impl Default for Leds {
    fn default() -> Self {
        Self::new()
    }
}

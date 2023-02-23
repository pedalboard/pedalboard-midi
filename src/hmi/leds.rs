use smart_leds::RGB8;

const NUM_LEDS: usize = 10;

#[derive(Debug, Clone, Copy)]
pub enum Led {
    A,
    B,
    C,
    D,
    E,
    F,
    Mode,
    Mon,
    L48V,
    Clip,
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

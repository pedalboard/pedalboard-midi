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
    Fill(RGB8, u8), // color, count (0-12)
}

#[derive(Copy, Clone)]
pub struct LedRing {
    rotation: u8,
    animation: Animation,
}

impl LedRing {
    pub fn new(rotation: u8) -> Self {
        LedRing {
            rotation,
            animation: Animation::Off,
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
                        data[(self.rotation as usize + LEDS_PER_RING - i) % LEDS_PER_RING] =
                            crate::loudness::loudness_color(reference);
                    }
                }
                data
            }
            Animation::Fill(c, count) => {
                let mut data = [RGB8::default(); LEDS_PER_RING];
                for i in 0..(count as usize).min(LEDS_PER_RING) {
                    data[(self.rotation as usize + i) % LEDS_PER_RING] = c;
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
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ledring_default_off() {
        let mut ring = LedRing::default();
        let data = ring.animate();
        for led in data.iter() {
            assert_eq!(*led, RGB8::default());
        }
    }

    #[test]
    fn test_ledring_on() {
        let mut ring = LedRing::default();
        let color = RGB8::new(255, 0, 0);
        ring.set(Animation::On(color));
        let data = ring.animate();
        for led in data.iter() {
            assert_eq!(*led, color);
        }
    }

    #[test]
    fn test_ledring_flash_clears() {
        let mut ring = LedRing::default();
        let color = RGB8::new(0, 255, 0);
        ring.set(Animation::Flash(color));

        let data = ring.animate();
        assert_eq!(data[0], color);

        let data = ring.animate();
        assert_eq!(data[0], RGB8::default());
    }

    #[test]
    fn test_ledring_toggle() {
        let mut ring = LedRing::default();
        let color = RGB8::new(0, 0, 255);

        ring.set(Animation::Toggle(color, false));
        let data = ring.animate();
        assert_eq!(data[0], RGB8::default());

        ring.set(Animation::Toggle(color, false));
        let data = ring.animate();
        assert_eq!(data[0], color);
    }
}

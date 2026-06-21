use smart_leds::RGB8;
pub const LEDS_PER_RING: usize = 12;

pub type LedData = [RGB8; LEDS_PER_RING];

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum Animation {
    On(RGB8),
    Off,
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

    /// Render current state to pixel buffer (no side effects).
    pub fn render(&self) -> LedData {
        match self.animation {
            Animation::On(c) => [c; LEDS_PER_RING],
            Animation::Off => [RGB8::default(); LEDS_PER_RING],
            Animation::Fill(c, count) => {
                let mut data = [RGB8::default(); LEDS_PER_RING];
                for i in 0..(count as usize).min(LEDS_PER_RING) {
                    data[(self.rotation as usize + LEDS_PER_RING - i) % LEDS_PER_RING] = c;
                }
                data
            }
        }
    }

    pub fn set(&mut self, a: Animation) {
        self.animation = a;
    }
}

impl Default for LedRing {
    fn default() -> Self {
        Self::new(8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ledring_default_off() {
        let ring = LedRing::default();
        let data = ring.render();
        for led in data.iter() {
            assert_eq!(*led, RGB8::default());
        }
    }

    #[test]
    fn test_ledring_on() {
        let mut ring = LedRing::default();
        let color = RGB8::new(255, 0, 0);
        ring.set(Animation::On(color));
        let data = ring.render();
        for led in data.iter() {
            assert_eq!(*led, color);
        }
    }

    #[test]
    fn test_ledring_fill() {
        let mut ring = LedRing::default();
        let color = RGB8::new(0, 255, 0);
        ring.set(Animation::Fill(color, 3));
        let data = ring.render();
        // With rotation=8, filled LEDs are at indices 8, 7, 6
        assert_eq!(data[8], color);
        assert_eq!(data[7], color);
        assert_eq!(data[6], color);
        assert_eq!(data[5], RGB8::default());
    }
}

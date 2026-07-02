use crate::ledring::{render_ring, Animation, LedRing, RingAnimation, LEDS_PER_RING};
use smart_leds::RGB8;

const NUM_LEDS: usize = 2;
const NUM_LED_RINGS: usize = 8;
pub const LED_OUTPUTS: usize = NUM_LEDS + NUM_LED_RINGS * LEDS_PER_RING;

pub type LedData = [RGB8; LED_OUTPUTS];

/// Map preset index to Mode LED color for bank indication.
pub fn preset_color(index: u8) -> RGB8 {
    const COLORS: [RGB8; 8] = [
        RGB8 { r: 0, g: 255, b: 0 }, // 0: green
        RGB8 { r: 0, g: 0, b: 255 }, // 1: blue
        RGB8 { r: 255, g: 0, b: 0 }, // 2: red
        RGB8 {
            r: 255,
            g: 255,
            b: 0,
        }, // 3: yellow
        RGB8 {
            r: 0,
            g: 255,
            b: 255,
        }, // 4: cyan
        RGB8 {
            r: 255,
            g: 0,
            b: 255,
        }, // 5: magenta
        RGB8 {
            r: 255,
            g: 128,
            b: 0,
        }, // 6: orange
        RGB8 {
            r: 255,
            g: 255,
            b: 255,
        }, // 7: white
    ];
    COLORS[(index as usize) % COLORS.len()]
}

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

/// Events sent to the led_out task. Single ownership — no locks needed.
#[derive(Debug, Clone, Copy)]
pub enum LedEvent {
    /// Set a ring animation.
    SetRing(LedRings, RingAnimation),
    /// Set all 8 rings at once (PE mode).
    SetAllRings([RingAnimation; 8]),
    /// Set a single LED (Mode/Mon) to a color or off.
    SetSingle(Led, Option<RGB8>),
    /// Flash a single LED for `duration_ticks` frames then auto-clear.
    Flash(Led, RGB8, u8),
    /// MIDI clock tick (24ppqn). When received, animations sync to BPM.
    BpmTick,
    /// Reactive LED: set heatmap on button ring (index 0-5, fill 0-12).
    SetReactiveRing(usize, u8),
}

pub struct Leds {
    singles: [Option<RGB8>; NUM_LEDS],
    ledrings: [LedRing; NUM_LED_RINGS],
    flash_ticks: [u8; NUM_LEDS],
    /// Rings controlled by reactive LED (SetReactiveRing) — skipped by SetAllRings.
    reactive: [bool; NUM_LED_RINGS],
    pub buffer: LedData,
    tick: u16,
    bpm_tick: u16,
    bpm_active: bool,
}

impl Leds {
    pub const fn new() -> Self {
        Leds {
            singles: [None; NUM_LEDS],
            ledrings: [LedRing::new(8); NUM_LED_RINGS],
            flash_ticks: [0; NUM_LEDS],
            reactive: [false; NUM_LED_RINGS],
            buffer: [RGB8 { r: 0, g: 0, b: 0 }; LED_OUTPUTS],
            tick: 0,
            bpm_tick: 0,
            bpm_active: false,
        }
    }

    /// Process an event (state change).
    pub fn handle_event(&mut self, event: LedEvent) {
        match event {
            LedEvent::SetRing(r, anim) => {
                self.ledrings[r as usize].set(anim);
            }
            LedEvent::SetAllRings(anims) => {
                const RING_ORDER: [LedRings; 8] = [
                    LedRings::A,
                    LedRings::B,
                    LedRings::C,
                    LedRings::D,
                    LedRings::E,
                    LedRings::F,
                    LedRings::Vol,
                    LedRings::Gain,
                ];
                for (i, anim) in anims.iter().enumerate() {
                    let ring_idx = RING_ORDER[i] as usize;
                    if !self.reactive[ring_idx] {
                        self.ledrings[ring_idx].set(*anim);
                    }
                }
            }
            LedEvent::SetSingle(led, color) => {
                self.singles[led as usize] = color;
                self.flash_ticks[led as usize] = 0;
            }
            LedEvent::Flash(led, color, duration) => {
                self.singles[led as usize] = Some(color);
                self.flash_ticks[led as usize] = duration;
            }
            LedEvent::BpmTick => {
                self.bpm_active = true;
                self.bpm_tick = self.bpm_tick.wrapping_add(1);
            }
            LedEvent::SetReactiveRing(btn_idx, fill) => {
                use crate::ledring::{Modifier, Renderer, RingAnimation};
                const BUTTON_RINGS: [LedRings; 6] = [
                    LedRings::A,
                    LedRings::B,
                    LedRings::C,
                    LedRings::D,
                    LedRings::E,
                    LedRings::F,
                ];
                if btn_idx < BUTTON_RINGS.len() {
                    let ring_idx = BUTTON_RINGS[btn_idx] as usize;
                    self.reactive[ring_idx] = true;
                    self.ledrings[ring_idx].set(RingAnimation {
                        renderer: Renderer::Heatmap(fill),
                        modifier: Modifier::Solid,
                    });
                }
            }
        }
    }

    /// Advance global animation tick. Call at 50Hz (or BPM-derived rate).
    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        // Count down flash timers
        for (i, ticks) in self.flash_ticks.iter_mut().enumerate() {
            if *ticks > 0 {
                *ticks -= 1;
                if *ticks == 0 {
                    self.singles[i] = None;
                }
            }
        }
    }

    /// Render current state into internal buffer, return reference.
    pub fn render(&mut self) -> &LedData {
        // Use BPM-synced tick when MIDI clock is running, else free-running
        let anim_tick = if self.bpm_active {
            self.bpm_tick
        } else {
            self.tick
        };

        for (ring_index, ring) in self.ledrings.iter().enumerate() {
            for (led_index, pixel) in render_ring(ring, anim_tick).into_iter().enumerate() {
                self.buffer[ring_index * LEDS_PER_RING + led_index] = pixel;
            }
        }

        for (i, color) in self.singles.iter().enumerate() {
            let led = NUM_LED_RINGS * LEDS_PER_RING + i;
            self.buffer[led] = color.unwrap_or_default();
        }

        &self.buffer
    }

    pub fn set_single(&mut self, l: Led, color: Option<RGB8>) {
        self.singles[l as usize] = color;
    }

    /// Set ring using legacy Animation enum (bridges existing call sites).
    pub fn set_ledring(&mut self, a: Animation, r: LedRings) {
        self.ledrings[r as usize].set(a.to_ring_animation());
    }

    /// Set ring using new RingAnimation directly.
    pub fn set_ring_animation(&mut self, anim: RingAnimation, r: LedRings) {
        self.ledrings[r as usize].set(anim);
    }
}

impl Default for Leds {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledring::{Modifier, Renderer, Rgb};
    use smart_leds::colors::*;

    #[test]
    fn test_leds_default_all_off() {
        let leds = Leds::new();
        let data = leds.render();
        for led in data.iter() {
            assert_eq!(*led, RGB8::default());
        }
    }

    #[test]
    fn test_set_single_on() {
        let mut leds = Leds::new();
        leds.handle_event(LedEvent::SetSingle(Led::Mon, Some(RED)));
        let data = leds.render();
        let mon_index = NUM_LED_RINGS * LEDS_PER_RING + Led::Mon as usize;
        assert_eq!(data[mon_index], RED);
    }

    #[test]
    fn test_set_single_off() {
        let mut leds = Leds::new();
        leds.handle_event(LedEvent::SetSingle(Led::Mon, Some(RED)));
        leds.handle_event(LedEvent::SetSingle(Led::Mon, None));
        let data = leds.render();
        let mon_index = NUM_LED_RINGS * LEDS_PER_RING + Led::Mon as usize;
        assert_eq!(data[mon_index], RGB8::default());
    }

    #[test]
    fn test_flash_auto_clears() {
        let mut leds = Leds::new();
        leds.handle_event(LedEvent::Flash(Led::Mon, BLUE, 3));
        let mon_index = NUM_LED_RINGS * LEDS_PER_RING + Led::Mon as usize;
        assert_eq!(leds.render()[mon_index], BLUE);
        leds.tick();
        leds.tick();
        assert_eq!(leds.render()[mon_index], BLUE);
        leds.tick(); // 3rd tick → clears
        assert_eq!(leds.render()[mon_index], RGB8::default());
    }

    #[test]
    fn test_tick_advances_blink() {
        let mut leds = Leds::new();
        leds.handle_event(LedEvent::SetRing(
            LedRings::A,
            RingAnimation {
                renderer: Renderer::Solid(Rgb::new(255, 0, 0)),
                modifier: Modifier::Blink,
            },
        ));
        let ring_start = LedRings::A as usize * LEDS_PER_RING;
        let on_val = leds.render()[ring_start];
        for _ in 0..12 {
            leds.tick();
        }
        let off_val = leds.render()[ring_start];
        assert_ne!(on_val, off_val);
    }

    #[test]
    fn test_set_all_rings() {
        let mut leds = Leds::new();
        let mut anims = [RingAnimation::off(); 8];
        anims[0] = RingAnimation::solid(Rgb::new(0, 255, 0));
        leds.handle_event(LedEvent::SetAllRings(anims));
        // Ring A = index 7 in enum, so check its position
        let ring_a_start = LedRings::A as usize * LEDS_PER_RING;
        assert_ne!(leds.render()[ring_a_start], RGB8::default());
    }
}

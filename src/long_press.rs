use crate::events::Edge;

const LONG_PRESS_TICKS: u16 = 500; // 500ms at 1ms poll rate

/// Detects long-press gestures. Returns LongPress on hold > threshold,
/// ShortPress on release before threshold.
pub struct LongPressDetector {
    held_ticks: u16,
    active: bool,
    fired: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    ShortPress,
    LongPress,
}

impl LongPressDetector {
    pub fn new() -> Self {
        Self {
            held_ticks: 0,
            active: false,
            fired: false,
        }
    }

    /// Call every tick with the current button edge (if any).
    /// Returns a gesture when detected.
    pub fn update(&mut self, edge: Option<Edge>) -> Option<Gesture> {
        match edge {
            Some(Edge::Activate) => {
                self.active = true;
                self.held_ticks = 0;
                self.fired = false;
                None
            }
            Some(Edge::Deactivate) => {
                self.active = false;
                if self.fired {
                    // Long press already handled, suppress short press
                    None
                } else {
                    Some(Gesture::ShortPress)
                }
            }
            None if self.active && !self.fired => {
                self.held_ticks = self.held_ticks.saturating_add(1);
                if self.held_ticks >= LONG_PRESS_TICKS {
                    self.fired = true;
                    Some(Gesture::LongPress)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Returns true while the button is held (suppresses normal events)
    pub fn is_active(&self) -> bool {
        self.active
    }
}

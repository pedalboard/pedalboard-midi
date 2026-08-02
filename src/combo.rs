//! Two-button combo detection with hold threshold.
//!
//! Tracks whether two named buttons are simultaneously held and fires once
//! when the combined hold duration reaches the threshold.
//!
//! # Usage
//!
//! ```
//! use pedalboard_midi::combo::ComboDetector;
//!
//! let mut combo = ComboDetector::new(3_000); // 3 second threshold
//!
//! // Simulate A pressed at t=0, F pressed at t=10, both held until t=3010
//! combo.update_a(true, 0);
//! combo.update_f(true, 10);
//! assert!(!combo.poll(2_999));
//! assert!(combo.poll(3_010)); // fires once
//! assert!(!combo.poll(3_100)); // does not fire again
//! ```

/// Detects a two-button combo hold gesture.
///
/// Fires once when both buttons have been held simultaneously for at least
/// `threshold_ms` milliseconds. Resets when either button is released.
#[derive(Debug, Clone)]
pub struct ComboDetector {
    a_held: bool,
    f_held: bool,
    /// Timestamp when both buttons first became simultaneously held.
    combo_start_ms: Option<u32>,
    /// Whether the combo has already fired in this hold cycle.
    fired: bool,
    threshold_ms: u32,
}

impl ComboDetector {
    pub fn new(threshold_ms: u32) -> Self {
        Self {
            a_held: false,
            f_held: false,
            combo_start_ms: None,
            fired: false,
            threshold_ms,
        }
    }

    /// Update state for button A. `pressed` = true on Activate, false on Deactivate.
    pub fn update_a(&mut self, pressed: bool, now_ms: u32) {
        self.a_held = pressed;
        self.on_button_change(now_ms);
    }

    /// Update state for button F. `pressed` = true on Activate, false on Deactivate.
    pub fn update_f(&mut self, pressed: bool, now_ms: u32) {
        self.f_held = pressed;
        self.on_button_change(now_ms);
    }

    /// Poll for a combo fire. Call once per loop tick with current time.
    /// Returns `true` exactly once when the threshold is reached.
    pub fn poll(&mut self, now_ms: u32) -> bool {
        if self.fired || !self.a_held || !self.f_held {
            return false;
        }
        match self.combo_start_ms {
            Some(start) if now_ms.wrapping_sub(start) >= self.threshold_ms => {
                self.fired = true;
                true
            }
            _ => false,
        }
    }

    /// Returns true while both buttons are currently held.
    pub fn both_held(&self) -> bool {
        self.a_held && self.f_held
    }

    fn on_button_change(&mut self, now_ms: u32) {
        if self.a_held && self.f_held {
            // Both now held — start timer if not already running.
            if self.combo_start_ms.is_none() {
                self.combo_start_ms = Some(now_ms);
                self.fired = false;
            }
        } else {
            // At least one released — reset.
            self.combo_start_ms = None;
            self.fired = false;
        }
    }
}

// Host-side tests for src/long_press.rs

#[path = "../../src/events.rs"]
mod events;

#[path = "../../src/long_press.rs"]
mod long_press;

use events::Edge;
use long_press::{Gesture, LongPressDetector};

#[test]
fn short_press_on_release_before_threshold() {
    let mut det = LongPressDetector::new();
    assert_eq!(det.update(Some(Edge::Activate)), None);
    // Hold for 100 ticks (< 500)
    for _ in 0..100 {
        assert_eq!(det.update(None), None);
    }
    assert_eq!(det.update(Some(Edge::Deactivate)), Some(Gesture::ShortPress));
}

#[test]
fn long_press_fires_at_threshold() {
    let mut det = LongPressDetector::new();
    det.update(Some(Edge::Activate));
    for _ in 0..499 {
        assert_eq!(det.update(None), None);
    }
    // 500th tick triggers long press
    assert_eq!(det.update(None), Some(Gesture::LongPress));
}

#[test]
fn release_after_long_press_suppressed() {
    let mut det = LongPressDetector::new();
    det.update(Some(Edge::Activate));
    for _ in 0..500 {
        det.update(None);
    }
    // Release should NOT produce ShortPress
    assert_eq!(det.update(Some(Edge::Deactivate)), None);
}

#[test]
fn no_double_fire_on_continued_hold() {
    let mut det = LongPressDetector::new();
    det.update(Some(Edge::Activate));
    for _ in 0..500 {
        det.update(None);
    }
    // Continue holding — should not fire again
    for _ in 0..500 {
        assert_eq!(det.update(None), None);
    }
}

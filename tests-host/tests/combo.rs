// Host-side tests for src/combo.rs

#[path = "../../src/combo.rs"]
mod combo;

use combo::ComboDetector;

// --- helpers ---

/// Press A then F, poll at `poll_ms`. Returns whether combo fired.
fn press_both_poll(threshold: u32, a_at: u32, f_at: u32, poll_ms: u32) -> bool {
    let mut c = ComboDetector::new(threshold);
    c.update_a(true, a_at);
    c.update_f(true, f_at);
    c.poll(poll_ms)
}

// --- basic fire / no-fire ---

#[test]
fn fires_when_both_held_at_threshold() {
    let mut c = ComboDetector::new(3_000);
    c.update_a(true, 0);
    c.update_f(true, 0);
    assert!(!c.poll(2_999));
    assert!(c.poll(3_000));
}

#[test]
fn does_not_fire_before_threshold() {
    assert!(!press_both_poll(3_000, 0, 0, 2_999));
}

#[test]
fn fires_exactly_at_threshold() {
    assert!(press_both_poll(3_000, 0, 0, 3_000));
}

#[test]
fn fires_after_threshold() {
    assert!(press_both_poll(3_000, 0, 0, 5_000));
}

// --- single fire guarantee ---

#[test]
fn fires_only_once_per_hold_cycle() {
    let mut c = ComboDetector::new(3_000);
    c.update_a(true, 0);
    c.update_f(true, 0);
    assert!(c.poll(3_000));
    assert!(!c.poll(3_001));
    assert!(!c.poll(4_000));
    assert!(!c.poll(10_000));
}

// --- release resets ---

#[test]
fn release_a_before_threshold_resets() {
    let mut c = ComboDetector::new(3_000);
    c.update_a(true, 0);
    c.update_f(true, 0);
    c.update_a(false, 1_000); // release A before threshold
    assert!(!c.poll(3_000));
    assert!(!c.poll(5_000));
}

#[test]
fn release_f_before_threshold_resets() {
    let mut c = ComboDetector::new(3_000);
    c.update_a(true, 0);
    c.update_f(true, 0);
    c.update_f(false, 1_000); // release F before threshold
    assert!(!c.poll(3_000));
}

#[test]
fn re_press_after_release_restarts_timer() {
    let mut c = ComboDetector::new(3_000);
    // First attempt — release A early
    c.update_a(true, 0);
    c.update_f(true, 0);
    c.update_a(false, 500);
    assert!(!c.poll(3_000)); // did not fire

    // Second attempt — hold all the way
    c.update_a(true, 4_000);
    assert!(!c.poll(6_999)); // 4000 + 3000 - 1
    assert!(c.poll(7_000)); // 4000 + 3000
}

// --- order independence ---

#[test]
fn timer_starts_when_second_button_pressed() {
    // A pressed at t=0, F pressed at t=1000 — threshold is 3s from when *both* held
    let mut c = ComboDetector::new(3_000);
    c.update_a(true, 0);
    // Only A held so far — should not fire at t=3000
    assert!(!c.poll(3_000));
    // F pressed at t=3500 — combo clock starts now
    c.update_f(true, 3_500);
    assert!(!c.poll(6_499)); // 3500 + 3000 - 1
    assert!(c.poll(6_500)); // 3500 + 3000
}

#[test]
fn f_before_a_works_identically() {
    let mut c = ComboDetector::new(3_000);
    c.update_f(true, 0);
    c.update_a(true, 100);
    assert!(!c.poll(3_099)); // 100 + 3000 - 1
    assert!(c.poll(3_100)); // 100 + 3000
}

// --- only one button held ---

#[test]
fn only_a_held_never_fires() {
    let mut c = ComboDetector::new(3_000);
    c.update_a(true, 0);
    assert!(!c.poll(10_000));
}

#[test]
fn only_f_held_never_fires() {
    let mut c = ComboDetector::new(3_000);
    c.update_f(true, 0);
    assert!(!c.poll(10_000));
}

// --- both_held helper ---

#[test]
fn both_held_reflects_state() {
    let mut c = ComboDetector::new(3_000);
    assert!(!c.both_held());
    c.update_a(true, 0);
    assert!(!c.both_held());
    c.update_f(true, 0);
    assert!(c.both_held());
    c.update_a(false, 100);
    assert!(!c.both_held());
}

// --- wrapping timestamps ---

#[test]
fn wrapping_timestamp_fires_correctly() {
    let mut c = ComboDetector::new(3_000);
    let start = u32::MAX - 1_000; // 1000ms before wrap
    c.update_a(true, start);
    c.update_f(true, start);
    // 1000ms held — not yet
    assert!(!c.poll(u32::MAX));
    // 2999ms held — still not
    assert!(!c.poll(1_998)); // wrapping: start + 2999 = u32::MAX - 1000 + 2999 = u32::MAX + 1999 wraps to 1998
    // 3000ms held — fires
    assert!(c.poll(1_999)); // wrapping: start + 3000 wraps to 1999
}

// --- toggle sequence (two fires) ---

#[test]
fn second_hold_fires_again_after_release_and_repress() {
    let mut c = ComboDetector::new(3_000);

    // First hold — fires at t=3000
    c.update_a(true, 0);
    c.update_f(true, 0);
    assert!(c.poll(3_000));

    // Release both
    c.update_a(false, 3_100);
    c.update_f(false, 3_100);

    // Second hold — fires at t=6100 + 3000 = 9100
    c.update_a(true, 6_100);
    c.update_f(true, 6_100);
    assert!(!c.poll(9_099));
    assert!(c.poll(9_100));
}

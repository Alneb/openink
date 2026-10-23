//! Scenario tests of the refresh policy (docs/EPDC-ENGINE.md). Times in ms.

use std::num::NonZeroU32;

use openink_core::UpdateMode;
use openink_epdc::{Engine, Intent, Millis, Policy, Reason, Rect};

fn screen() -> Rect {
    Rect::new(0, 0, 1860, 2480).unwrap() // Note Air 5C panel
}
fn r(l: i32, t: i32, rr: i32, b: i32) -> Rect {
    Rect::new(l, t, rr, b).unwrap()
}
fn engine() -> Engine {
    Engine::new(screen(), Policy::default())
}

#[test]
fn a_burst_coalesces_into_one_update() {
    let mut e = engine();
    e.request(r(0, 0, 100, 40), Intent::Ui, Millis(0));
    e.request(r(0, 50, 100, 90), Intent::Ui, Millis(5)); // 10 px away: within merge_gap
    assert!(e.poll(Millis(10)).is_empty(), "still inside the coalescing window");
    assert_eq!(e.next_deadline(), Some(Millis(25)));
    let out = e.poll(Millis(25));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].rect, r(0, 0, 100, 90));
    assert_eq!(out[0].mode, UpdateMode::GU);
    assert_eq!(out[0].reason, Reason::Request);
}

#[test]
fn different_modes_keep_their_order_when_they_overlap() {
    let mut e = engine();
    e.request(r(0, 0, 100, 100), Intent::Ui, Millis(0));
    e.request(r(50, 50, 150, 150), Intent::Motion, Millis(1));
    e.request(r(100, 100, 200, 200), Intent::Ui, Millis(2)); // must not jump ahead of the A2 one
    let out = e.poll(Millis(30));
    let modes: Vec<_> = out.iter().map(|u| u.mode).collect();
    assert_eq!(modes, [UpdateMode::GU, UpdateMode::A2_PERFORMANCE, UpdateMode::GU]);
}

#[test]
fn a_better_update_that_covers_an_older_one_absorbs_it() {
    let mut e = engine();
    e.request(r(10, 10, 50, 50), Intent::Motion, Millis(0));
    e.request(r(0, 0, 100, 100), Intent::Ui, Millis(1));
    let out = e.poll(Millis(30));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].mode, UpdateMode::GU);
}

#[test]
fn writing_holds_refreshes_until_the_pen_rests() {
    let mut e = engine();
    e.pen_down(Millis(0));
    e.request(r(0, 0, 100, 100), Intent::Ui, Millis(10));
    assert_eq!(e.next_deadline(), None, "nothing goes out under the pen");
    assert!(e.poll(Millis(1000)).is_empty());
    e.pen_up(Some(r(500, 500, 600, 520)), Millis(1000));
    assert!(e.poll(Millis(1200)).is_empty(), "the next stroke may still come");
    e.pen_down(Millis(1300)); // it does: keep holding
    e.pen_up(Some(r(600, 500, 700, 520)), Millis(1600));
    assert_eq!(e.next_deadline(), Some(Millis(2100)));
    let out = e.poll(Millis(2100));
    // The two strokes merged into one DU repaint; the UI update went with them.
    assert!(out.iter().any(|u| u.mode == UpdateMode::DU && u.rect == r(500, 500, 700, 520)));
    assert!(out.iter().any(|u| u.mode == UpdateMode::GU));
}

#[test]
fn a_long_writing_session_does_not_starve_the_ui() {
    let mut e = engine();
    e.request(r(0, 0, 10, 10), Intent::Ui, Millis(0));
    e.pen_down(Millis(1));
    e.pen_up(None, Millis(4000)); // > max_defer_ms after the request
    assert_eq!(e.next_deadline(), Some(Millis(4000)), "sent at pen-up, no repaint delay");
}

#[test]
fn ghosts_are_cleaned_where_they_are_once_idle() {
    let mut e = engine();
    let area = r(0, 0, 200, 200);
    for i in 0..6 {
        e.request(area, Intent::Motion, Millis(i * 100));
        assert_eq!(e.poll(Millis(i * 100 + 25)).len(), 1);
    }
    // ledger at 6.0 = threshold; quiet since 525 → cleanup at 2025
    assert_eq!(e.next_deadline(), Some(Millis(2025)));
    assert!(e.poll(Millis(2000)).is_empty());
    let out = e.poll(Millis(2025));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].reason, Reason::Cleanup);
    assert_eq!(out[0].mode, Intent::Clean.mode());
    assert_eq!(out[0].rect, r(0, 0, 256, 256), "tile-aligned, not the whole screen");
    assert_eq!(e.next_deadline(), None, "ledger reset");
}

#[test]
fn no_cleanup_while_writing() {
    let mut e = engine();
    for i in 0..10 {
        e.request(r(0, 0, 64, 64), Intent::Motion, Millis(i));
    }
    e.poll(Millis(100));
    e.pen_down(Millis(200));
    assert_eq!(e.next_deadline(), None);
    assert!(e.poll(Millis(100_000)).is_empty());
}

#[test]
fn gc_interval_promotes_every_nth_partial() {
    let mut e = engine();
    e.set_gc_every(NonZeroU32::new(3));
    let mut reasons = Vec::new();
    for i in 0..6u64 {
        e.request(r(0, 0, 10, 10), Intent::Ui, Millis(i * 100));
        reasons.extend(e.poll(Millis(i * 100 + 25)).into_iter().map(|u| u.reason));
    }
    use Reason::*;
    assert_eq!(reasons, [Request, Request, GcInterval, Request, Request, GcInterval]);
}

#[test]
fn scope_mode_overrides_but_raw_passes_through() {
    let mut e = engine();
    e.set_scope_mode(Some(UpdateMode::A2_PERFORMANCE));
    e.request(r(0, 0, 10, 10), Intent::Ui, Millis(0));
    e.request(r(500, 500, 510, 510), Intent::Raw(UpdateMode::GC), Millis(0));
    let out = e.poll(Millis(25));
    assert_eq!(out[0].mode, UpdateMode::A2_PERFORMANCE);
    assert_eq!(out[1].mode, UpdateMode::GC);
}

#[test]
fn offscreen_requests_are_dropped_and_partial_ones_clipped() {
    let mut e = engine();
    e.request(r(5000, 5000, 5100, 5100), Intent::Ui, Millis(0));
    assert_eq!(e.next_deadline(), None);
    e.request(r(1800, 2400, 1900, 2500), Intent::Ui, Millis(0));
    assert_eq!(e.poll(Millis(25))[0].rect, r(1800, 2400, 1860, 2480));
}

#[test]
fn flush_skips_the_waits_but_not_the_pen() {
    let mut e = engine();
    e.request(r(0, 0, 10, 10), Intent::Ui, Millis(0));
    assert_eq!(e.flush(Millis(1)).len(), 1, "no coalescing wait");
    e.pen_down(Millis(2));
    e.request(r(0, 0, 10, 10), Intent::Ui, Millis(3));
    assert!(e.flush(Millis(4)).is_empty(), "never under the pen");
    e.pen_up(None, Millis(5));
    assert_eq!(e.flush(Millis(6)).len(), 1, "no pen-up wait");
}

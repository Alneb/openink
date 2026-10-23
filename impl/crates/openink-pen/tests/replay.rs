//! Replays a real Note Air 5C stylus trace (recorded with `pen-dump`) through the decoder.
//! Expected counts come from the raw events (BTN_TOUCH / BTN_TOOL_BRUSH toggles), not from the
//! decoder, so a decoder regression cannot silently move both sides.

use openink_pen::decode::{PenAction, Tool};
use openink_pen::trace::{self, Trace};
use openink_pen::PenDecoder;

const TRACE: &str = include_str!("data/note-air-5c-strokes.trace");

fn load() -> Trace {
    trace::parse(TRACE).expect("fixture parses")
}

fn raw_toggles(t: &Trace, code: u16, value: i32) -> usize {
    t.events.iter().filter(|e| e.kind == 1 && e.code == code && e.value == value).count()
}

#[test]
fn real_trace_decodes_into_well_formed_strokes() {
    let t = load();
    assert!(t.events.len() > 10_000, "fixture truncated: {} events", t.events.len());
    let mut dec = PenDecoder::new(t.pressure);
    let out: Vec<_> = t.events.iter().filter_map(|e| dec.feed(*e)).collect();
    let count = |a| out.iter().filter(|e| e.action == a).count();

    let downs = raw_toggles(&t, 0x14a, 1);
    assert_eq!(downs, 17);
    assert_eq!(count(PenAction::Down), downs);
    assert_eq!(count(PenAction::Up), raw_toggles(&t, 0x14a, 0));
    // One session enters range and touches in the same report: Down without a HoverEnter.
    assert_eq!(count(PenAction::HoverLeave), raw_toggles(&t, 0x142, 0));
    assert_eq!(count(PenAction::HoverEnter), raw_toggles(&t, 0x142, 1) - 1);
    assert_eq!(count(PenAction::ForcedUp), 0);

    // Order: Down, Move*, Up; contact always has pressure; hover never does.
    let mut touching = false;
    for e in &out {
        assert_eq!(e.tool, Tool::Pen);
        match e.action {
            PenAction::Down => { assert!(!touching); touching = true; }
            PenAction::Up => { assert!(touching); touching = false; }
            PenAction::Move => assert!(touching),
            PenAction::Hover | PenAction::HoverEnter | PenAction::HoverLeave => assert!(!touching),
            PenAction::ForcedUp => unreachable!(),
        }
        if matches!(e.action, PenAction::Down | PenAction::Move) {
            assert!(e.pressure > 0.0 && e.pressure <= 1.0, "pressure {} at {}", e.pressure, e.time_us);
        }
    }
    assert!(!touching, "trace ends mid-stroke");
}

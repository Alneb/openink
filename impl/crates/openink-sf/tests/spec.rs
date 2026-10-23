//! Every request's Parcel shape (arg kinds in order, reply) must match what the firmware's
//! ViewUpdateHelper writes, as extracted into impl/spec/sf_protocol.json.

use openink_core::{PenCommand, RegionMode, StrokeStyle, UpdateMode};
use openink_sf::request::{self as r, Arg, Pid, Reply, Request, ScreenRect};
use serde_json::Value;

fn spec() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../spec/sf_protocol.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("spec missing")).unwrap()
}

fn kind(a: &Arg) -> &'static str {
    match a {
        Arg::I32(_) => "Int",
        Arg::F32(_) => "Float",
        Arg::I32s(_) => "IntArray",
    }
}

fn all() -> Vec<Request> {
    let a = ScreenRect::new(0, 0, 10, 10);
    vec![
        r::refresh(a, UpdateMode::DU),
        r::wait_for_update_finished(),
        r::set_pen_state(PenCommand::Start, Pid(1)),
        r::pen_state(),
        r::set_region_limit(&[a]),
        r::set_region_exclude(&[a]),
        r::set_region_mode(RegionMode::Multi),
        r::set_stroke_width(2.0),
        r::set_stroke_color(-16777216),
        r::set_stroke_style(StrokeStyle::Pencil),
        r::set_brush_raw_drawing(true),
        r::set_eraser_raw_drawing(false, 5),
        r::enable_post(true, Pid(1)),
        r::pen_up(),
        r::handwriting_repaint(a, false),
    ]
}

#[test]
fn requests_match_firmware_parcels() {
    let spec = spec();
    let methods = spec["methods"].as_array().unwrap();
    let mut checked = 0;
    for req in all() {
        let code_name = openink_core::Code::ALL.iter().find(|(_, c)| *c == req.code).unwrap().0;
        // A method name can have several overloads; one with this code and arg shape must exist.
        let ours: Vec<&str> = req.args.iter().map(kind).collect();
        let hit = methods.iter().any(|m| {
            m["method"] == req.method
                && m["codes"].as_array().unwrap().iter().any(|c| c == code_name)
                && m["writes"].as_array().unwrap().iter().map(|w| w.as_str().unwrap().split('(').next().unwrap()).eq(ours.iter().copied())
                && (m["reply"].as_array().unwrap().is_empty() == (req.reply == Reply::None))
        });
        assert!(hit, "{} ({code_name}) {ours:?} matches no spec overload", req.method);
        checked += 1;
    }
    assert_eq!(checked, 15);
}

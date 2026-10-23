//! Conformance against the spec extracted from the firmware (impl/spec/sf_protocol.json).

use openink_core::{Code, UpdateMode, Waveform};
use serde_json::Value;

fn spec() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../spec/sf_protocol.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("spec missing")).unwrap()
}

#[test]
fn codes_match_spec_both_ways() {
    let s = spec();
    let codes = s["codes"].as_object().unwrap();
    assert!(codes.len() >= 100, "spec has only {} codes", codes.len());
    assert_eq!(codes.len(), Code::ALL.len(), "regenerate sf_codes.rs (gen_sf_codes.py)");
    for (name, code) in Code::ALL {
        assert_eq!(codes[*name].as_u64(), Some(*code as u64), "{name}");
        assert_eq!(Code::from_u32(*code as u32), Some(*code));
    }
}

#[test]
fn named_modes_match_spec() {
    let c = spec()["constants"].clone();
    let mut checked = 0;
    for (name, mode) in UpdateMode::NAMED {
        let v = c[*name].as_i64().unwrap_or_else(|| panic!("{name} not in spec")) as i32;
        assert_eq!(mode.to_onyx(), v, "{name}");
        assert_eq!(UpdateMode::from_onyx(v), Ok(*mode), "{name} decode");
        checked += 1;
    }
    // Shape check: every UI_*_MODE in the spec must be modelled, so a new one fails loudly.
    let spec_ui = c.as_object().unwrap().keys().filter(|k| k.starts_with("UI_") && k.ends_with("_MODE")).count();
    assert_eq!(checked - 1, spec_ui, "UI_*_MODE constants not modelled in UpdateMode::NAMED");
}

#[test]
fn waveform_nibbles_match_spec() {
    let c = spec()["constants"].clone();
    for (name, value) in c.as_object().unwrap() {
        let Some(wf) = name.strip_prefix("EINK_WAVEFORM_MODE_") else { continue };
        if wf == "MASK" {
            continue;
        }
        let n = value.as_u64().unwrap() as u8;
        assert!(Waveform::from_nibble(n).is_some(), "{name}={n} not modelled");
    }
}

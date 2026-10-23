//! The Rust recogniser must match the torch trainer: same features, same log-probabilities.
//! Fixture: `train_hwr.py fixture MODEL.pt tests/fixtures/parity.json FONT_DIR`, written from the same
//! model as weights/hwr.oisw (regenerate both together).

use openink_hwr::{features, Point, Recognizer};

/// The fixture records which feature variant the model reads.
fn deslant(f: &serde_json::Value) -> bool {
    f["deslant"].as_bool().unwrap_or(false)
}

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/parity.json")).unwrap()
}

fn floats(v: &serde_json::Value) -> Vec<Vec<f32>> {
    v.as_array().unwrap().iter().map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect()).collect()
}

fn strokes(v: &serde_json::Value) -> Vec<Vec<Point>> {
    v.as_array().unwrap().iter().map(|s| floats(s).iter().map(|p| [p[0], p[1]]).collect()).collect()
}

#[test]
fn features_match_python() {
    let fx = fixture();
    for case in fx["cases"].as_array().unwrap() {
        let want = floats(&case["features"]);
        let got = features(&strokes(&case["strokes"]), deslant(&fx));
        assert_eq!(got.len(), want.len(), "{}", case["text"]);
        for (g, w) in got.iter().zip(&want) {
            for (a, b) in g.iter().zip(w) {
                assert!((a - b).abs() < 1e-4, "{}: {a} vs {b}", case["text"]);
            }
        }
    }
}

#[test]
fn log_probs_match_torch() {
    let rec = Recognizer::from_bytes(include_bytes!("../weights/hwr.oisw")).unwrap();
    for case in fixture()["cases"].as_array().unwrap() {
        let want = floats(&case["log_probs"]);
        let got = rec.log_probs(&strokes(&case["strokes"]));
        assert_eq!(got.len(), want.len());
        let worst = got.iter().flatten().zip(want.iter().flatten()).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(worst < 2e-3, "{}: max |Δ log p| = {worst}", case["text"]);
    }
}

//! Runs the built-in recogniser on word crops from the user's notes (`data/hwr/words.json`, local only,
//! made by docs/scripts/hwr_words.py) and prints each crop's top candidates: a real-ink check.
//! usage: shape-crops words.json ID [ID...]      (points are [x, y, t_ms])
//! SHAPE_REJECT_MARGIN=100 shows the candidates the reject would otherwise hide.

use openink_shape::features::Sample;
use openink_shape::model::{Decoration, Primitive, Recognition};
use openink_shape::recognize::Recognizer;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 2 {
        eprintln!("usage: shape-crops words.json ID [ID...]");
        std::process::exit(2);
    }
    let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&a[0]).expect("words.json")).expect("json");
    let words = doc["words"].as_array().expect("words[]");
    let mut rec = Recognizer::builtin();
    if let Ok(m) = std::env::var("SHAPE_REJECT_MARGIN") {
        rec.reject_margin = m.parse().expect("SHAPE_REJECT_MARGIN");
    }
    for id in &a[1..] {
        let id: u64 = id.parse().expect("numeric id");
        let Some(w) = words.iter().find(|w| w["id"].as_u64() == Some(id)) else {
            println!("{id}: not found");
            continue;
        };
        let ink: Vec<Vec<Sample>> = w["strokes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                s.as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|p| Some(Sample { x: p[0].as_f64()? as f32, y: p[1].as_f64()? as f32, t: p.get(2)?.as_f64()? as f32 }))
                    .collect()
            })
            .collect();
        let what = match rec.recognize(&ink) {
            Recognition::Shape(c) => c
                .iter()
                .take(8)
                .map(|c| {
                    let heads: Vec<String> = c.primitives.iter().flat_map(|p| match p {
                        Primitive::Line { d1, d2, .. } | Primitive::Arc { d1, d2, .. } => [*d1, *d2],
                    }).filter(|d| *d != Decoration::None).map(|d| format!("{d:?}")).collect();
                    format!("{:?} {:.2}{}", c.class, c.cost, if heads.is_empty() { String::new() } else { format!(" [{}]", heads.join(" ")) })
                })
                .collect::<Vec<_>>()
                .join(", "),
            Recognition::ScratchOut { .. } => "ScratchOut".into(),
            Recognition::Rejected => "Rejected".into(),
        };
        println!("{id} ({} strokes): {what}", ink.len());
    }
}

//! End-to-end accuracy of the recogniser on fresh synthetic ink (seeds unseen in training).
//! usage: shape-eval WEIGHTS.oisw [N] [SEED] [REJECT_MARGIN] [RESIDUAL_WEIGHT]

use openink_shape::model::{Family, Recognition, ShapeClass};
use openink_shape::nn::Models;
use openink_shape::recognize::Recognizer;
use openink_shape::synth::{example, Rng};
use std::collections::BTreeMap;

fn name(r: &Recognition) -> String {
    match r {
        Recognition::Shape(c) => format!("{:?}", c[0].class),
        Recognition::ScratchOut { .. } => "ScratchOut".into(),
        Recognition::Rejected => "Rejected".into(),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: shape-eval WEIGHTS.oisw [N] [SEED]");
    let n: usize = args.next().map_or(5000, |s| s.parse().expect("N"));
    let seed: u64 = args.next().map_or(3, |s| s.parse().expect("SEED"));
    let models = Models::from_bytes(&std::fs::read(&path).expect("read weights")).expect("parse weights");
    let mut rec = Recognizer::new(models);
    if let Some(m) = args.next() {
        rec.reject_margin = m.parse().expect("REJECT_MARGIN");
    }
    if let Some(w) = args.next() {
        rec.residual_weight = w.parse().expect("RESIDUAL_WEIGHT");
    }
    let mut rng = Rng::new(seed);
    let (mut exact, mut family) = (0, 0);
    let mut per: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut confusions: BTreeMap<(String, String), usize> = BTreeMap::new();
    for _ in 0..n {
        let ex = example(&mut rng);
        // Scratch-outs carry no class, only the family.
        let truth = match (ex.class, ex.family) {
            (Some(c), _) => format!("{c:?}"),
            (None, Family::ScratchOut) => "ScratchOut".into(),
            (None, _) => "Rejected".into(),
        };
        let got = rec.recognize(&ex.ink);
        let got_name = name(&got);
        let got_family = match &got {
            Recognition::Shape(c) => Some(c[0].class.family()),
            Recognition::ScratchOut { .. } => Some(ShapeClass::ScratchOut.family()),
            Recognition::Rejected => None,
        };
        let ok = got_name == truth;
        exact += ok as usize;
        family += (got_family.unwrap_or(Family::Reject) == ex.family) as usize;
        let e = per.entry(truth.clone()).or_default();
        e.0 += ok as usize;
        e.1 += 1;
        if !ok {
            *confusions.entry((truth, got_name)).or_default() += 1;
        }
    }
    println!(
        "exact class {:.3}  family {:.3}  (n = {n}, seed {seed}, reject margin {}, residual weight {})",
        exact as f32 / n as f32,
        family as f32 / n as f32,
        rec.reject_margin,
        rec.residual_weight
    );
    for (k, (ok, tot)) in &per {
        println!("  {k:24} {:.3}  ({ok}/{tot})", *ok as f32 / *tot as f32);
    }
    let mut c: Vec<_> = confusions.into_iter().collect();
    c.sort_by_key(|e| std::cmp::Reverse(e.1));
    println!("top confusions (truth → got):");
    for ((t, g), k) in c.iter().take(20) {
        println!("  {t:24} → {g:24} {k}");
    }
}

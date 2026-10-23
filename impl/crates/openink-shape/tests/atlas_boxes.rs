//! Regression cases from Atlas (docs/memory/FROM-ATLAS.md, 2026-10-17 night): long thin boxes, upright
//! and turned, with evenly sampled sides or at constant pen speed, must come out as rectangles that keep
//! their turn. Same generator as Atlas's `shape_openink.rs` tests (`polyline()` / `wobble()`).

use openink_shape::features::Sample;
use openink_shape::geom::{pt, Pt};
use openink_shape::model::{Primitive, Recognition, ShapeClass};
use openink_shape::recognize::Recognizer;

fn wobble(i: usize) -> f32 {
    let i = i as f32;
    ((i * 1.7).sin() + (i * 0.37).cos()) * 0.9
}

/// Corners joined by `per_side` evenly spaced points each (or points `spacing` apart), closed, wobbled,
/// turned by `turn` about (100, 100); t = i·dt ms.
fn box_stroke(corners: &[(f32, f32)], turn: f32, dt: f32, spacing: Option<f32>) -> Vec<Sample> {
    let mut pts: Vec<Pt> = Vec::new();
    for k in 0..corners.len() {
        let (a, b) = (corners[k], corners[(k + 1) % corners.len()]);
        let (a, b) = (pt(a.0, a.1), pt(b.0, b.1));
        let n = spacing.map_or(30, |s| (a.dist(b) / s).round().max(1.0) as usize);
        pts.extend((0..n).map(|j| a.lerp(b, j as f32 / n as f32)));
    }
    pts.push(pt(corners[0].0, corners[0].1));
    let c = pt(100.0, 100.0);
    pts.iter()
        .enumerate()
        .map(|(i, p)| {
            let q = p.sub(c).rotate(turn).add(c);
            Sample { x: q.x + wobble(i), y: q.y + wobble(i + 3), t: i as f32 * dt }
        })
        .collect()
}

const THIN: [(f32, f32); 4] = [(0.0, 60.0), (200.0, 60.0), (200.0, 140.0), (0.0, 140.0)];

/// The long side's direction, mod π.
fn long_side_angle(prims: &[Primitive]) -> f32 {
    let mut best = (0.0, 0.0);
    for p in prims {
        if let Primitive::Line { p1, p2, .. } = p {
            let l = p1.dist(*p2);
            if l > best.0 {
                best = (l, p2.sub(*p1).angle());
            }
        }
    }
    best.1.rem_euclid(std::f32::consts::PI)
}

fn check(name: &str, ink: Vec<Sample>, turn: f32) {
    let rec = Recognizer::builtin();
    match rec.recognize(&[ink]) {
        Recognition::Shape(c) => {
            assert_eq!(c[0].class, ShapeClass::Rectangle, "{name}: got {:?} (cost {})", c[0].class, c[0].cost);
            let got = long_side_angle(&c[0].primitives);
            let want = turn.rem_euclid(std::f32::consts::PI);
            let d = (got - want).abs().min(std::f32::consts::PI - (got - want).abs());
            assert!(d < 0.12, "{name}: long side at {got:.3} rad, want {want:.3}");
        }
        other => panic!("{name}: got {other:?}"),
    }
}

#[test]
fn a_long_upright_box_is_a_rectangle() {
    for dt in [4.0, 8.0, 16.0] {
        for turn in [0.0, 0.1] {
            check(&format!("200x80 turn {turn} dt {dt}"), box_stroke(&THIN, turn, dt, None), turn);
        }
    }
}

#[test]
fn a_tilted_box_keeps_its_turn() {
    for dt in [4.0, 8.0, 16.0] {
        for turn in [0.2, 0.3, 0.5, 0.7, 0.785] {
            check(&format!("200x80 turn {turn} dt {dt}"), box_stroke(&THIN, turn, dt, None), turn);
        }
    }
}

#[test]
fn a_constant_speed_box_is_a_rectangle() {
    for turn in [0.0, 0.3, 0.6] {
        check(&format!("200x80 constant speed turn {turn}"), box_stroke(&THIN, turn, 8.0, Some(4.0)), turn);
    }
}

#[test]
fn the_wider_box_still_works() {
    let wide = [(0.0, 0.0), (200.0, 0.0), (200.0, 120.0), (0.0, 120.0)];
    check("200x120", box_stroke(&wide, 0.0, 8.0, None), 0.0);
}

/// Prints the family probabilities and candidate costs for the hard cases (`--ignored --nocapture`).
#[test]
#[ignore]
fn debug_costs() {
    let mut rec = Recognizer::builtin();
    rec.reject_margin = 100.0;
    for (turn, sp) in [(0.0, None), (0.1, None), (0.7, None), (0.0, Some(4.0))] {
        let ink = vec![box_stroke(&THIN, turn, 4.0, sp)];
        let norm = openink_shape::features::Norm::new(&ink).unwrap();
        let probs = rec.models.classifier.probabilities(&openink_shape::features::stroke_features(&norm));
        let p: Vec<String> = probs.iter().map(|p| format!("{p:.3}")).collect();
        if let Recognition::Shape(c) = rec.recognize(&ink) {
            let s: Vec<String> = c.iter().map(|c| format!("{:?} {:.2}", c.class, c.cost)).collect();
            println!("turn {turn} sp {sp:?}: probs [{}]\n   {}", p.join(" "), s.join(", "));
        }
    }
}

/// Atlas's `wobble()` flips sign almost every sample: on closely spaced points it read as curvature and
/// the classifier called every upright box an ellipse. Features are now computed on lightly smoothed ink.
#[test]
fn sample_level_zigzag_is_not_curvature() {
    for h in [40.0f32, 80.0, 120.0] {
        for turn in [0.0f32, 0.05, 0.2, 0.4, 0.7] {
            let c = [(0.0, 100.0 - h / 2.0), (200.0, 100.0 - h / 2.0), (200.0, 100.0 + h / 2.0), (0.0, 100.0 + h / 2.0)];
            check(&format!("200x{h} turn {turn}"), box_stroke(&c, turn, 4.0, None), turn);
        }
    }
}


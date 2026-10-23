//! Atlas's 2026-10-18 cases (docs/memory/FROM-ATLAS.md): small regular polygons, hexagons split into 7-gons, and
//! one-stroke arrows with a V or closed head. Same generator as Atlas: points 8 ms apart, wobble on x (i) and y (i + 3).
use openink_shape::features::Sample;
use openink_shape::geom::{pt, Pt};
use openink_shape::model::{Recognition, ShapeClass};
use openink_shape::recognize::Recognizer;

fn wobble(i: usize) -> f32 {
    let i = i as f32;
    ((i * 1.7).sin() + (i * 0.37).cos()) * 0.9
}

fn stroke(pts: &[Pt]) -> Vec<Sample> {
    pts.iter().enumerate().map(|(i, p)| Sample { x: p.x + wobble(i), y: p.y + wobble(i + 3), t: i as f32 * 8.0 }).collect()
}

fn polyline(corners: &[Pt], per: usize) -> Vec<Pt> {
    let mut v = Vec::new();
    for w in corners.windows(2) {
        v.extend((0..per).map(|j| w[0].lerp(w[1], j as f32 / per as f32)));
    }
    v.push(*corners.last().unwrap());
    v
}

fn ngon(n: usize, r: f32, turn: f32) -> Vec<Pt> {
    let c = pt(300.0, 300.0);
    let mut v: Vec<Pt> = (0..n).map(|k| c.add(Pt::unit(turn + k as f32 * std::f32::consts::TAU / n as f32).scale(r))).collect();
    v.push(v[0]);
    polyline(&v, (r / 4.0) as usize)
}

fn report(name: &str, r: &Recognition) -> String {
    match r {
        Recognition::Shape(c) => format!("{name}: {:?} ({} prims)", c[0].class, c[0].primitives.len()),
        other => format!("{name}: {other:?}"),
    }
}

#[test]
#[ignore]
fn survey() {
    let rec = Recognizer::builtin();
    for (n, r, turn) in [(6, 60.0, 0.5), (5, 60.0, 0.5), (6, 60.0, 0.0), (6, 120.0, 0.0), (6, 120.0, 0.2), (6, 250.0, 0.5)] {
        println!("{}", report(&format!("{n}-gon r {r} turn {turn}"), &rec.recognize(&[stroke(&ngon(n, r, turn))])));
    }
    for (len, turn) in [(120.0f32, 0.0f32), (250.0, 2.5), (180.0, 1.0)] {
        let tail = pt(100.0, 100.0);
        let d = Pt::unit(turn);
        let tip = tail.add(d.scale(len));
        let b1 = tip.sub(d.rotate(0.5).scale(0.2 * len));
        let b2 = tip.sub(d.rotate(-0.5).scale(0.2 * len));
        let v = polyline(&[tail, tip, b1, tip, b2], 30);
        println!("{}", report(&format!("V arrow len {len} turn {turn}"), &rec.recognize(&[stroke(&v)])));
        let v2 = polyline(&[tail, tip, b1, b2, tip], 30);
        println!("{}", report(&format!("triangle-head arrow len {len} turn {turn}"), &rec.recognize(&[stroke(&v2)])));
    }
}

/// Small regular polygons stay polygons.
#[test]
fn small_hexagon_is_a_polygon() {
    let rec = Recognizer::builtin();
    let r = rec.recognize(&[stroke(&ngon(6, 60.0, 0.5))]);
    assert!(matches!(&r, Recognition::Shape(c) if c[0].class == ShapeClass::Polygon && c[0].primitives.len() == 6), "{}", report("6-gon", &r));
}

/// Atlas's 7-gon split: a hexagon side broken at an 8–11° bend is still one side.
#[test]
fn large_hexagons_keep_six_sides() {
    let rec = Recognizer::builtin();
    for (r, turn) in [(120.0, 0.0), (120.0, 0.2), (250.0, 0.5)] {
        let res = rec.recognize(&[stroke(&ngon(6, r, turn))]);
        assert!(matches!(&res, Recognition::Shape(c) if c[0].class == ShapeClass::Polygon && c[0].primitives.len() == 6), "{}", report(&format!("6-gon r {r} turn {turn}"), &res));
    }
}

/// A closed head drawn without lifting the pen (tail → tip → b1 → b2 → tip) is a verified triangle head.
#[test]
fn one_stroke_closed_head_is_an_arrow() {
    let rec = Recognizer::builtin();
    for (len, turn) in [(120.0f32, 0.0f32), (250.0, 2.5), (180.0, 1.0)] {
        let tail = pt(100.0, 100.0);
        let d = Pt::unit(turn);
        let tip = tail.add(d.scale(len));
        let (b1, b2) = (tip.sub(d.rotate(0.5).scale(0.2 * len)), tip.sub(d.rotate(-0.5).scale(0.2 * len)));
        let r = rec.recognize(&[stroke(&polyline(&[tail, tip, b1, b2, tip], 30))]);
        assert!(matches!(&r, Recognition::Shape(c) if c[0].class == ShapeClass::Arrow), "{}", report(&format!("len {len} turn {turn}"), &r));
    }
}

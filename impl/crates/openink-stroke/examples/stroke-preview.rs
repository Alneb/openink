//! Render a real pen trace with every pen, for eyeballing the geometry on the host:
//!   cargo run -p openink-stroke --example stroke-preview -- [trace] [out.pgm] [scale]
//! Defaults: the openink-pen fixture, target/stroke-preview.pgm, scale 1. One panel per pen,
//! top to bottom in `PenKind::ALL` order, each cropped to the strokes' bounding box.

use openink_pen::{trace, PenAction, PenDecoder};
use openink_stroke::raster::Gray;
use openink_stroke::{PenConfig, PenKind, Sample, Stroke};

/// The Note Air 5C panel and its digitizer → screen map (docs/OPEN-STACK.md: fitted, rms 0 px).
const SCREEN_W: f32 = 1860.0;
const SCREEN_H: f32 = 2480.0;
const DPI: f32 = 300.0;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let default_trace = concat!(env!("CARGO_MANIFEST_DIR"), "/../openink-pen/tests/data/note-air-5c-strokes.trace");
    let path = args.get(1).filter(|s| !s.is_empty()).map_or(default_trace, String::as_str);
    let out = args.get(2).map_or("target/stroke-preview.pgm", String::as_str);
    let scale: f32 = args.get(3).map_or(1.0, |s| s.parse().expect("scale"));

    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let t = trace::parse(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
    let (kx, ky) = (SCREEN_H / t.x.max as f32, SCREEN_W / t.y.max as f32);
    let mut dec = PenDecoder::new(t.pressure);
    let mut strokes: Vec<Vec<Sample>> = Vec::new();
    let mut t0 = 0i64;
    let mut tilt_max = 0f32;
    // A recording started with the pen on the glass begins with moves: skip them until a down.
    let mut open = false;
    let mut orphan_moves = 0usize;
    for e in t.events.iter().filter_map(|r| dec.feed(*r)) {
        let s = |t0: i64| Sample {
            x: (SCREEN_W - e.y as f32 * ky) * scale,
            y: e.x as f32 * kx * scale,
            pressure: e.pressure,
            tilt_x: e.tilt_x as f32,
            tilt_y: e.tilt_y as f32,
            t_ms: (e.time_us - t0) as f32 / 1000.0,
        };
        match e.action {
            PenAction::Down => {
                t0 = e.time_us;
                strokes.push(vec![s(t0)]);
                open = true;
            }
            PenAction::Move | PenAction::Up | PenAction::ForcedUp if open => {
                tilt_max = tilt_max.max((e.tilt_x as f32).hypot(e.tilt_y as f32));
                strokes.last_mut().expect("open stroke").push(s(t0));
                open = e.action == PenAction::Move;
            }
            PenAction::Move | PenAction::Up | PenAction::ForcedUp => orphan_moves += 1,
            _ => {}
        }
    }
    if orphan_moves > 0 {
        eprintln!("[stroke-preview] skipped {orphan_moves} events of a stroke already in progress when the trace began");
    }
    if strokes.is_empty() {
        eprintln!("[stroke-preview] FAILED: {path} has no pen-down, nothing to draw");
        std::process::exit(1);
    }
    let pts = strokes.iter().flatten();
    let (l, t_, r, b) = pts.fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |(l, t, r, b), p| (l.min(p.x), t.min(p.y), r.max(p.x), b.max(p.y)));
    let margin = 60.0 * scale;
    let (pw, ph) = ((r - l + 2.0 * margin) as usize, (b - t_ + 2.0 * margin) as usize);
    let mut img = Gray::new(pw, ph * PenKind::ALL.len());
    for (i, kind) in PenKind::ALL.into_iter().enumerate() {
        let mut cfg = PenConfig::knote2_default(kind, DPI);
        cfg.width *= scale;
        let dy = i as f32 * ph as f32;
        for st in &strokes {
            let moved: Vec<Sample> = st.iter().map(|p| Sample { x: p.x - l + margin, y: p.y - t_ + margin + dy, ..*p }).collect();
            img.draw(&Stroke::new(kind, cfg).render(&moved));
        }
        // A rule between panels.
        for x in 0..pw {
            img.px[(i * ph) * pw + x] = 0.5;
        }
        println!("panel {i}: {:<9} width {:.1} px", kind.name(), cfg.width);
    }
    std::fs::write(out, img.to_pgm()).unwrap_or_else(|e| panic!("{out}: {e}"));
    println!("{} strokes, {} points, max tilt {tilt_max:.0}; wrote {out} ({pw}x{})", strokes.len(), strokes.iter().map(Vec::len).sum::<usize>(), ph * PenKind::ALL.len());
}

//! Renders a samples file (`x y pressure tiltX tiltY tMs` per line) with one pen and prints the ink polygons,
//! one per line (`n x0 y0 …`), or the dabs (`5 x y size angle alpha`), to compare with libneo's own output
//! (impl/java/neo-oracle).
//! usage: stroke-dump PEN_CODE SAMPLES [width] [smoothLevel] [dpi] [minWidth] [pressureSensitivity]
//! CHARCOAL_PGM=out.pgm writes the dithered charcoal bitmap instead.
use openink_stroke::{PenConfig, PenKind, Sample, Stroke};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let kind = PenKind::from_code(a[0].parse().expect("pen code")).expect("pen");
    let f = |i: usize, d: f32| a.get(i).map_or(d, |v| v.parse().expect("number"));
    let mut cfg = PenConfig::knote2_default(kind, 300.0);
    cfg.width = f(2, cfg.width);
    cfg.smooth_level = f(3, cfg.smooth_level);
    cfg.dpi = f(4, 320.0);
    cfg.min_width = f(5, 0.001);
    cfg.pressure_sensitivity = f(6, cfg.pressure_sensitivity);
    let samples: Vec<Sample> = std::fs::read_to_string(&a[1])
        .expect("samples")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let v: Vec<f32> = l.split_whitespace().map(|x| x.parse().unwrap()).collect();
            Sample { x: v[0], y: v[1], pressure: v[2], tilt_x: v[3], tilt_y: v[4], t_ms: v[5] }
        })
        .collect();
    let ink = Stroke::new(kind, cfg).render(&samples);
    if let Ok(path) = std::env::var("CHARCOAL_PGM") {
        // The dithered charcoal bitmap (raster::charcoal_bitmap), as a PGM, ink black.
        let (_, _, w, h, px) = openink_stroke::raster::charcoal_bitmap(&ink.data);
        let mut out = format!("P5 {w} {h} 255\n").into_bytes();
        out.extend(px.iter().map(|v| 255 - v));
        std::fs::write(path, out).expect("write pgm");
        return;
    }
    let d = &ink.data;
    if ink.output == openink_stroke::pen::Output::Dabs {
        // One dab per line, as the oracle prints them: `5 x y size angle alpha`.
        for dab in d.chunks(5) {
            println!("5 {}", dab.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" "));
        }
        return;
    }
    let mut i = 0;
    while i < d.len() {
        let n = d[i] as usize;
        let pts = &d[i + 1..i + 1 + 2 * n];
        println!("{n} {}", pts.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" "));
        i += 1 + 2 * n;
    }
}

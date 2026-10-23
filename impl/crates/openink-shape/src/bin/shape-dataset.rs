//! Training-data generator for openink-shape (docs/SHAPE-ENGINE.md, "Training data").
//!
//! Writes flat little-endian arrays that numpy reads with `fromfile`:
//!   clf_x.f32 [N, 40]  clf_y.u8 [N] (Family index)  clf_src.u8 [N] (Source)
//!   seg_x.f32 [P, 10]  seg_y.u8 [P] (PointLabel)    seg_len.u32 [S] (points per sequence)
//! Only synthetic ink carries per-point labels, so only it feeds the segmenter.
//!
//! usage: shape-dataset --out DIR [--seed S] [--synth N] [--quickdraw DIR --qd-skip K --qd-take N]
//!                      [--notes notes.json --notes-take N]

use openink_shape::features::{point_features, stroke_features, Norm, Sample};
use openink_shape::geom::{path_len, pt, BBox, Pt};
use openink_shape::model::Family;
use openink_shape::synth::{example, Rng};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
#[repr(u8)]
enum Source {
    Synthetic = 0,
    QuickDraw = 1,
    Notes = 2,
}

/// Quick, Draw! categories we use and the family each one teaches.
const QD: &[(&str, Family)] = &[
    ("circle", Family::Ellipse),
    ("square", Family::Polygon),
    ("triangle", Family::Polygon),
    ("hexagon", Family::Polygon),
    ("octagon", Family::Polygon),
    ("diamond", Family::Polygon),
    ("line", Family::Line),
    ("zigzag", Family::Polyline),
    ("squiggle", Family::Reject),
];

struct Out {
    clf_x: BufWriter<File>,
    clf_y: BufWriter<File>,
    clf_src: BufWriter<File>,
    seg_x: BufWriter<File>,
    seg_y: BufWriter<File>,
    seg_len: BufWriter<File>,
    counts: [[usize; Family::COUNT]; 3],
}

impl Out {
    fn create(dir: &Path) -> std::io::Result<Out> {
        std::fs::create_dir_all(dir)?;
        let f = |n: &str| File::create(dir.join(n)).map(BufWriter::new);
        Ok(Out {
            clf_x: f("clf_x.f32")?,
            clf_y: f("clf_y.u8")?,
            clf_src: f("clf_src.u8")?,
            seg_x: f("seg_x.f32")?,
            seg_y: f("seg_y.u8")?,
            seg_len: f("seg_len.u32")?,
            counts: [[0; Family::COUNT]; 3],
        })
    }

    fn classifier(&mut self, n: &Norm, fam: Family, src: Source) -> std::io::Result<()> {
        for v in stroke_features(n) {
            self.clf_x.write_all(&v.to_le_bytes())?;
        }
        self.clf_y.write_all(&[fam.index() as u8])?;
        self.clf_src.write_all(&[src as u8])?;
        self.counts[src as usize][fam.index()] += 1;
        Ok(())
    }

    fn segmenter(&mut self, n: &Norm, label_of: impl Fn(usize, usize) -> u8) -> std::io::Result<()> {
        let feats = point_features(n);
        for (f, rp) in feats.iter().zip(&n.pts) {
            for v in f {
                self.seg_x.write_all(&v.to_le_bytes())?;
            }
            self.seg_y.write_all(&[label_of(rp.stroke, rp.src)])?;
        }
        self.seg_len.write_all(&(feats.len() as u32).to_le_bytes())
    }

    fn finish(mut self) -> std::io::Result<[[usize; Family::COUNT]; 3]> {
        for w in [&mut self.clf_x, &mut self.clf_y, &mut self.clf_src, &mut self.seg_x, &mut self.seg_y, &mut self.seg_len] {
            w.flush()?;
        }
        Ok(self.counts)
    }
}

fn synthetic(out: &mut Out, seed: u64, n: usize) -> std::io::Result<()> {
    let mut rng = Rng::new(seed);
    let mut made = 0;
    while made < n {
        let ex = example(&mut rng);
        let Some(norm) = Norm::new(&ex.ink) else { continue };
        out.classifier(&norm, ex.family, Source::Synthetic)?;
        out.segmenter(&norm, |s, i| ex.labels[s][i] as u8)?;
        made += 1;
    }
    Ok(())
}

/// Quick, Draw! raw ndjson: `drawing` = [[xs, ys, ts], ...] per stroke. Only `recognized` drawings.
fn quickdraw(out: &mut Out, dir: &Path, skip: usize, take: usize) -> std::io::Result<()> {
    for &(cat, fam) in QD {
        let path = dir.join(format!("{cat}.ndjson"));
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("quickdraw: skip {}: {e}", path.display());
                continue;
            }
        };
        let (mut seen, mut used) = (0, 0);
        for line in BufReader::new(file).lines() {
            if used >= take {
                break;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line?) else { continue };
            if v["recognized"] != serde_json::Value::Bool(true) {
                continue;
            }
            seen += 1;
            if seen <= skip {
                continue;
            }
            let Some(strokes) = v["drawing"].as_array() else { continue };
            let ink: Vec<Vec<Sample>> = strokes.iter().filter_map(qd_stroke).collect();
            if let Some(norm) = Norm::new(&ink) {
                out.classifier(&norm, fam, Source::QuickDraw)?;
                used += 1;
            }
        }
        eprintln!("quickdraw: {cat:9} → {fam:?}: {used}");
    }
    Ok(())
}

fn qd_stroke(s: &serde_json::Value) -> Option<Vec<Sample>> {
    let a = s.as_array()?;
    let (xs, ys, ts) = (a.first()?.as_array()?, a.get(1)?.as_array()?, a.get(2)?.as_array()?);
    let v: Vec<Sample> = xs
        .iter()
        .zip(ys)
        .zip(ts)
        .filter_map(|((x, y), t)| Some(Sample { x: x.as_f64()? as f32, y: y.as_f64()? as f32, t: t.as_f64()? as f32 }))
        .collect();
    (!v.is_empty()).then_some(v)
}

/// The user's handwriting as Reject: single strokes and runs of 2–3 consecutive strokes (words).
/// Strokes that could be honest shapes are skipped, so an unsnapped underline or a drawn box
/// never teaches "lines are not shapes": nearly straight strokes and closed loops.
fn notes(out: &mut Out, path: &Path, take: usize, seed: u64) -> std::io::Result<()> {
    let doc: serde_json::Value = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    let mut pen: Vec<Vec<Sample>> = Vec::new();
    for d in doc["documents"].as_array().into_iter().flatten() {
        for p in d["pages"].as_array().into_iter().flatten() {
            for s in p["strokes"].as_array().into_iter().flatten() {
                if s["shapeType"].as_i64() != Some(5) {
                    continue; // 5 = free pen stroke
                }
                let pts: Vec<Sample> = s["points"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|q| {
                        let q = q.as_array()?;
                        Some(Sample { x: q.first()?.as_f64()? as f32, y: q.get(1)?.as_f64()? as f32, t: q.get(3)?.as_f64()? as f32 })
                    })
                    .collect();
                if pts.len() >= 2 {
                    pen.push(pts);
                }
            }
        }
    }
    let mut rng = Rng::new(seed ^ 0x6e6f746573);
    let (mut used, mut skipped, mut i) = (0, 0, 0);
    while used < take && i < pen.len() {
        let k = (1 + rng.int(0, 2)).min(pen.len() - i);
        let ink = &pen[i..i + k];
        i += k;
        if k == 1 && maybe_shape(&ink[0]) {
            skipped += 1;
            continue;
        }
        if let Some(norm) = Norm::new(ink) {
            out.classifier(&norm, Family::Reject, Source::Notes)?;
            used += 1;
        }
    }
    eprintln!("notes: {} pen strokes, {used} Reject examples, {skipped} shape-like strokes skipped", pen.len());
    Ok(())
}

fn maybe_shape(s: &[Sample]) -> bool {
    let p: Vec<Pt> = s.iter().map(|q| pt(q.x, q.y)).collect();
    let (len, diag) = (path_len(&p), BBox::of(&p).diag());
    let ends = p[0].dist(*p.last().unwrap());
    len <= 0.0 || ends / len > 0.9 || (len > 2.5 * diag && ends < 0.15 * diag)
}

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut out_dir, mut seed, mut synth_n) = (None::<PathBuf>, 1u64, 0usize);
    let (mut qd, mut qd_skip, mut qd_take) = (None::<PathBuf>, 0usize, usize::MAX);
    let (mut notes_path, mut notes_take) = (None::<PathBuf>, usize::MAX);
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| panic!("{a} needs a value"));
        match a.as_str() {
            "--out" => out_dir = Some(val().into()),
            "--seed" => seed = val().parse().expect("--seed"),
            "--synth" => synth_n = val().parse().expect("--synth"),
            "--quickdraw" => qd = Some(val().into()),
            "--qd-skip" => qd_skip = val().parse().expect("--qd-skip"),
            "--qd-take" => qd_take = val().parse().expect("--qd-take"),
            "--notes" => notes_path = Some(val().into()),
            "--notes-take" => notes_take = val().parse().expect("--notes-take"),
            _ => panic!("unknown argument {a} (see the header of shape-dataset.rs)"),
        }
    }
    let dir = out_dir.expect("--out DIR is required");
    let mut out = Out::create(&dir)?;
    eprintln!("synthetic: {synth_n} examples, seed {seed}");
    synthetic(&mut out, seed, synth_n)?;
    if let Some(q) = qd {
        quickdraw(&mut out, &q, qd_skip, qd_take)?;
    }
    if let Some(n) = notes_path {
        notes(&mut out, &n, notes_take, seed)?;
    }
    let counts = out.finish()?;
    for (src, c) in ["synthetic", "quickdraw", "notes"].iter().zip(counts) {
        eprintln!("{src:9}: {:?}", Family::ALL.iter().zip(c).filter(|(_, n)| *n > 0).collect::<Vec<_>>());
    }
    Ok(())
}

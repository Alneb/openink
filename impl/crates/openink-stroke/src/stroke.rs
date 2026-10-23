//! One stroke, incrementally: `Stroke::down` → `ActiveStroke::moved`* → `ActiveStroke::up`.
//! Each call returns only the new ink; earlier ink never changes, so a live view can draw it
//! once and keep it (the order is a typestate: `up` consumes the stroke).

use crate::brush::BrushStroke;
use crate::filter::Smoother;
use crate::marker::MarkerStroke;
use crate::geom::{self, DEG};
use crate::pen::{Output, PenConfig, PenKind};

/// One input point in screen px. `pressure` 0..1; tilt in degrees; `t_ms` since the stroke began
/// (relative, so f32 keeps sub-ms precision for the whole stroke).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub t_ms: f32,
}

impl Sample {
    pub fn is_finite(&self) -> bool {
        [self.x, self.y, self.pressure, self.tilt_x, self.tilt_y, self.t_ms].iter().all(|v| v.is_finite())
    }
}

/// New ink from one call. `data` layout depends on `output`: polygons (see [`geom::Polygons`])
/// for `Fill`/`Translucent`, `[x, y, size, angle_rad, alpha]` per dab for `Dabs`.
#[derive(Clone, Debug, PartialEq)]
pub struct Ink {
    pub output: Output,
    pub data: Vec<f32>,
    /// `[left, top, right, bottom]` of everything in `data`; `None` when empty.
    pub bounds: Option<[f32; 4]>,
}

impl Ink {
    fn new(output: Output) -> Self {
        Ink { output, data: Vec::new(), bounds: None }
    }

    pub(crate) fn empty_fill() -> Self {
        Ink::new(Output::Fill)
    }

    pub(crate) fn empty(output: Output) -> Self {
        Ink::new(output)
    }

    pub(crate) fn grow_by(&mut self, x: f32, y: f32, pad: f32) {
        self.grow(x, y, pad);
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    fn grow(&mut self, x: f32, y: f32, pad: f32) {
        let b = self.bounds.get_or_insert([x, y, x, y]);
        b[0] = b[0].min(x - pad);
        b[1] = b[1].min(y - pad);
        b[2] = b[2].max(x + pad);
        b[3] = b[3].max(y + pad);
    }

    /// Append `other` (same pen) to this ink.
    pub fn extend(&mut self, other: Ink) {
        debug_assert_eq!(self.output, other.output);
        if let Some([l, t, r, b]) = other.bounds {
            self.grow(l, t, 0.0);
            self.grow(r, b, 0.0);
        }
        self.data.extend(other.data);
    }
}

/// A smoothed point with its computed footprint.
#[derive(Clone, Copy, Debug)]
struct Node {
    x: f32,
    y: f32,
    /// Half width (outline pens), nib length (square), or dab size (textured pens).
    size: f32,
    /// Dab opacity 0..1 (textured pens only).
    alpha: f32,
    /// The filtered sample it came from and its travel: nodes inside a segment are computed from the spline of
    /// the samples, as libneo splines every channel before its width calculators run.
    src: Sample,
    step: f32,
}

/// libneo measures speed as travel per sample in 160-dpi px (`NeoPenConfig.dpi`).
fn to_160dpi(cfg: &PenConfig, px: f32) -> f32 {
    px * 160.0 / cfg.dpi
}
/// The square nib: knote2's `NeoSquarePen.defaultPenConfig` (angle 45°, ratio 10). libneo's 45° rises to the right
/// on screen (y grows downwards), so it is −45° here.
const NIB_ANGLE: f32 = -45.0 * DEG;
const NIB_RATIO: f32 = 10.0;
/// Thinnest outline half-width (px): libneo's `minWidth` default 0.001 would make zero-area polygons.
const MIN_HALF_PX: f32 = 0.25;

/// A stroke that has not touched down yet: the pen and its settings.
#[derive(Clone, Copy, Debug)]
pub struct Stroke {
    kind: PenKind,
    cfg: PenConfig,
}

/// A stroke between pen-down and pen-up.
pub struct ActiveStroke(Active);

/// The brush and the marker have their own libneo pipelines (crate::brush, crate::marker: raw points joined by
/// round-capped lines); every other pen runs the smoothing + spline one.
enum Active {
    Spline(Box<SplineStroke>),
    Brush(BrushStroke),
    Marker(MarkerStroke),
}

struct SplineStroke {
    kind: PenKind,
    cfg: PenConfig,
    smoother: Smoother,
    /// libneo runs the width calculators on the raw samples and then filters their output like the positions: the
    /// same FIR over (size, alpha), carried in a Sample's x and y.
    attr: Smoother,
    /// The last raw sample (the calculators' speed is raw travel).
    raw_last: Sample,
    /// The last three nodes (Catmull-Rom needs the one before the segment's start).
    prev2: Option<Node>,
    /// The newest node: its segment is drawn once the next one arrives (libneo's spline looks one point ahead).
    pending: Option<Node>,
    prev: Node,
    /// Travel of the last sample, px (libneo's per-sample velocity).
    step: f32,
    /// libneo's dab placement (pencil, charcoal) and angles; `Some` exactly for the dab pens.
    dabs: Option<DistanceDabs>,
}

/// libneo's `SplineDistanceInterpolator` (`FUN_00451010`) as the dab pens run it, and the pencil's dab angles
/// (`FUN_004580e0`: `java.util.Random`, seeded at pen-down, `nextInt(360)` degrees per dab).
struct DistanceDabs {
    /// Dab spacing as a fraction of the dab size: the pencil's brushSpacing 0.25, charcoal V2's fixed 0.1
    /// (`FUN_0044c8ec`).
    spacing: f32,
    rng: JavaRandom,
    /// The last dab placed: x, y, size.
    last: Option<(f32, f32, f32)>,
}

/// knote2's NeoPenConfig.brushSpacing for the pencil; charcoal V2 hard-codes 0.1.
const PENCIL_SPACING: f32 = 0.25;
const CHARCOAL_SPACING: f32 = 0.1;

/// `java.util.Random`'s 48-bit LCG (libneo `FUN_00470cb8` / `FUN_00470d34` are its constructor and `nextInt`).
struct JavaRandom(u64);

impl JavaRandom {
    const MUL: u64 = 0x5DEE_CE66D;
    const MASK: u64 = (1 << 48) - 1;

    fn new(seed: i64) -> Self {
        JavaRandom((seed as u64 ^ Self::MUL) & Self::MASK)
    }

    fn next_int(&mut self, bound: u32) -> u32 {
        loop {
            self.0 = (self.0.wrapping_mul(Self::MUL).wrapping_add(0xB)) & Self::MASK;
            let bits = ((self.0 >> 17) & 0x7fff_ffff) as u32;
            if bound.is_power_of_two() {
                return ((bits as u64 * bound as u64) >> 31) as u32;
            }
            let v = bits % bound;
            if (bits - v).checked_add(bound - 1).is_some_and(|x| x < 1 << 31) {
                return v;
            }
        }
    }
}

impl DistanceDabs {
    /// The seed libneo computes from the down point: 13·⌊1024·p⌋ (+ two point fields that are 0 for pen input).
    fn new(spacing: f32, down: Sample) -> Self {
        DistanceDabs { spacing, rng: JavaRandom::new(13 * (down.pressure * 1024.0) as i64), last: None }
    }

    fn for_pen(kind: PenKind, down: Sample) -> Option<Self> {
        match kind {
            PenKind::Pencil => Some(DistanceDabs::new(PENCIL_SPACING, down)),
            PenKind::Charcoal => Some(DistanceDabs::new(CHARCOAL_SPACING, down)),
            _ => None,
        }
    }
}

impl Stroke {
    pub fn new(kind: PenKind, cfg: PenConfig) -> Self {
        Stroke { kind, cfg: cfg.sanitized() }
    }

    pub fn kind(&self) -> PenKind {
        self.kind
    }

    /// Pen-down: starts the stroke and returns its first ink (a dot, so a tap leaves a mark).
    /// `s` must be finite (later samples may not be: those are skipped).
    pub fn down(self, s: Sample) -> (ActiveStroke, Ink) {
        debug_assert!(s.is_finite(), "pen-down sample {s:?}");
        match self.kind {
            PenKind::Brush => {
                let (b, ink) = BrushStroke::down(&self.cfg, s);
                return (ActiveStroke(Active::Brush(b)), ink);
            }
            PenKind::Marker => {
                let (m, ink) = MarkerStroke::down(&self.cfg, s);
                return (ActiveStroke(Active::Marker(m)), ink);
            }
            _ => {}
        }
        let mut smoother = Smoother::new(self.cfg.smooth_level);
        let mut attr = Smoother::new(self.cfg.smooth_level);
        let first = smoother.push(s).unwrap_or(s);
        let raw = node_for(self.kind, &self.cfg, s, 0.0);
        let a0 = attr.push(attr_sample(raw)).unwrap_or(attr_sample(raw));
        let node = Node { x: first.x, y: first.y, size: a0.x, alpha: a0.y, src: first, step: 0.0 };
        let mut a = SplineStroke {
            kind: self.kind,
            cfg: self.cfg,
            smoother,
            attr,
            raw_last: s,
            prev2: None,
            pending: None,
            prev: node,
            step: 0.0,
            dabs: DistanceDabs::for_pen(self.kind, s),
        };
        let mut ink = Ink::new(self.kind.output());
        // A dab pen's first dab comes from its first segment (t = 0), as in libneo.
        if a.dabs.is_none() {
            a.dot(&mut ink, node);
        }
        (ActiveStroke(Active::Spline(Box::new(a))), ink)
    }

    /// Render a whole recorded stroke (redraw from storage): down, all moves, up.
    pub fn render(self, samples: &[Sample]) -> Ink {
        // Leading non-finite samples cannot start a stroke; later ones are skipped by the smoother.
        let start = samples.iter().position(Sample::is_finite).unwrap_or(samples.len());
        let Some((&first, rest)) = samples[start..].split_first() else { return Ink::new(self.kind.output()) };
        let (mut a, mut ink) = self.down(first);
        match rest.split_last() {
            Some((&last, mid)) => {
                ink.extend(a.moved(mid));
                ink.extend(a.up(last));
            }
            None => ink.extend(a.up(first)),
        }
        ink
    }
}

impl ActiveStroke {
    pub fn kind(&self) -> PenKind {
        match &self.0 {
            Active::Spline(s) => s.kind,
            Active::Brush(_) => PenKind::Brush,
            Active::Marker(_) => PenKind::Marker,
        }
    }

    pub fn moved(&mut self, samples: &[Sample]) -> Ink {
        match &mut self.0 {
            Active::Spline(s) => s.moved(samples),
            Active::Brush(b) => {
                let mut ink = Ink::empty_fill();
                for &p in samples.iter().filter(|p| p.is_finite()) {
                    ink.extend(b.moved(p));
                }
                ink
            }
            Active::Marker(m) => {
                let mut ink = Ink::empty(Output::Translucent);
                for &p in samples.iter().filter(|p| p.is_finite()) {
                    ink.extend(m.to(p));
                }
                ink
            }
        }
    }

    /// Pen-up (or a forced release): the stroke's last ink.
    pub fn up(self, s: Sample) -> Ink {
        match self.0 {
            Active::Spline(a) => a.up(s),
            // A non-finite up point ends the brush where it is.
            Active::Brush(b) => {
                let at = if s.is_finite() { s } else { b.last_sample() };
                b.up(at)
            }
            Active::Marker(mut m) => {
                let at = if s.is_finite() { s } else { m.last_sample() };
                m.to(at)
            }
        }
    }
}

impl SplineStroke {
    fn moved(&mut self, samples: &[Sample]) -> Ink {
        let mut ink = Ink::new(self.kind.output());
        for &s in samples {
            if !s.is_finite() {
                continue;
            }
            let raw = self.raw_node(s);
            if let (Some(p), Some(a)) = (self.smoother.push(s), self.attr.push(attr_sample(raw))) {
                self.advance(&mut ink, p, a);
            }
        }
        ink
    }

    /// The calculator's output for a raw sample, with its raw travel as the speed.
    fn raw_node(&mut self, s: Sample) -> Node {
        let step = (s.x - self.raw_last.x).hypot(s.y - self.raw_last.y);
        self.raw_last = s;
        node_for(self.kind, &self.cfg, s, step)
    }

    /// Pen-up (or a forced release): the filter runs out to the up point, the last segment closes on it, and
    /// the stroke ends.
    fn up(mut self, s: Sample) -> Ink {
        let mut ink = Ink::new(self.kind.output());
        if s.is_finite() {
            let raw = self.raw_node(s);
            let attrs = self.attr.finish(attr_sample(raw));
            for (p, a) in self.smoother.finish(s).into_iter().zip(attrs) {
                self.advance(&mut ink, p, a);
            }
        }
        if let Some(last) = self.pending.take() {
            self.segment(&mut ink, last, last); // libneo duplicates the end point
        }
        ink
    }

    /// `p`: the filtered position, `a`: the filtered (size, alpha).
    fn advance(&mut self, ink: &mut Ink, p: Sample, a: Sample) {
        let last = self.pending.unwrap_or(self.prev);
        self.step = (p.x - last.x).hypot(p.y - last.y);
        let node = Node { x: p.x, y: p.y, size: a.x, alpha: a.y, src: p, step: self.step };
        if let Some(to) = self.pending.replace(node) {
            self.segment(ink, to, node);
        }
    }

    /// Ink from `self.prev` to `to` along libneo's uniform Catmull-Rom (SplineProducer: the basis
    /// matrix `_INIT_3` builds), through `prev2` before and `after` beyond; then `to` becomes `prev`.
    fn segment(&mut self, ink: &mut Ink, to: Node, after: Node) {
        let a = self.prev;
        let before = self.prev2.unwrap_or(a);
        let quad = [before.src, a.src, to.src, after.src];
        let at = |t: f32| catmull_rom(&quad, t);
        // libneo's width channel: the calculators run on the input points and their widths are splined too, so a
        // node inside the segment takes the spline of the knot widths (in libneo's units: a diameter, or the nib).
        if self.dabs.is_some() {
            self.dab_segment(ink, [before, a, to, after]);
            self.prev2 = Some(a);
            self.prev = to;
            return;
        }
        let k = self.width_units();
        let sizes = [before.size, a.size, to.size, after.size].map(|v| v * k);
        let width = |t: f32| cr1(sizes, t);
        let mut ts = Vec::new();
        subdivide(&at, &width, 0.0, 1.0, &mut ts, 0);
        let mut last = a;
        for &t in ts.iter().chain(std::iter::once(&1.0)) {
            let cur = if t >= 1.0 {
                to
            } else {
                let p = at(t);
                let n = node_for(self.kind, &self.cfg, p, to.step);
                Node { size: if k > 0.0 { width(t) / k } else { n.size }, ..n }
            };
            self.piece(ink, last, cur);
            last = cur;
        }
        self.prev2 = Some(a);
        self.prev = to;
    }

    /// Dabs along one spline segment: n + 1 candidates at t = j/n, n = ⌊k·(d / min size) / spacing⌋ + 1 with
    /// k = clamp(10·spacing, 1, 10) and d the knot distance; a candidate becomes a dab when it is at least spacing ·
    /// the mean of its and the last dab's size away from the last dab. t = 0 is only a candidate before the first.
    fn dab_segment(&mut self, ink: &mut Ink, q: [Node; 4]) {
        let [_, a, to, _] = q;
        let quad = q.map(|n| n.src);
        let (sizes, alphas) = (q.map(|n| n.size), q.map(|n| n.alpha));
        let d = (to.x - a.x).hypot(to.y - a.y);
        let spacing = self.dabs.as_ref().expect("dab state").spacing;
        let k = (spacing * 10.0).clamp(1.0, 10.0);
        let n = ((k * (d / a.size.min(to.size)) / spacing) as i32 + 1) as u32;
        for j in 0..=n {
            let t = j as f32 / n as f32;
            let p = catmull_rom(&quad, t);
            let size = cr1(sizes, t);
            let st = self.dabs.as_mut().expect("dab state");
            let far = match st.last {
                None => true,
                Some(_) if t == 0.0 => false,
                Some((x, y, s)) => {
                    let need = (s + size) * 0.5 * spacing;
                    (p.x - x).powi(2) + (p.y - y).powi(2) >= need * need
                }
            };
            if far {
                st.last = Some((p.x, p.y, size));
                let angle = st.rng.next_int(360) as f32 * DEG;
                let alpha = cr1(alphas, t).clamp(0.0, 1.0);
                ink.data.extend([p.x, p.y, size, angle, alpha]);
                ink.grow(p.x, p.y, size * 0.75);
            }
        }
    }

    /// `Node::size` → libneo's width channel: outline pens keep a half width (libneo a diameter), the square nib its
    /// length; 0 for the dab pens, whose calculators are not channel-checked yet.
    fn width_units(&self) -> f32 {
        match (self.kind, self.kind.output()) {
            (_, Output::Dabs) => 0.0,
            (PenKind::Square, _) => 1.0,
            _ => 2.0,
        }
    }

    fn piece(&mut self, ink: &mut Ink, a: Node, b: Node) {
        match self.kind.output() {
            Output::Fill | Output::Translucent if self.kind == PenKind::Square => {
                geom::nib_sweep(&mut ink.data, (a.x, a.y, a.size), (b.x, b.y, b.size), NIB_ANGLE, NIB_RATIO);
                ink.grow(a.x, a.y, a.size / 2.0 + 1.0);
                ink.grow(b.x, b.y, b.size / 2.0 + 1.0);
            }
            Output::Fill | Output::Translucent => {
                geom::octagon_capsule(&mut ink.data, (a.x, a.y, a.size), (b.x, b.y, b.size));
                ink.grow(a.x, a.y, a.size + 1.0);
                ink.grow(b.x, b.y, b.size + 1.0);
            }
            Output::Dabs => unreachable!("dab pens are placed by dab_segment"),
        }
    }

    /// The pen-down mark of an outline pen (dab pens get theirs from the first segment).
    fn dot(&mut self, ink: &mut Ink, n: Node) {
        self.piece(ink, n, n);
    }
}

/// A node's (size, alpha) as a Sample for the attribute filter.
fn attr_sample(n: Node) -> Sample {
    Sample { x: n.size, y: n.alpha, pressure: 0.0, tilt_x: 0.0, tilt_y: 0.0, t_ms: n.src.t_ms }
}

/// Width/opacity model, ported from libneo_pen.so's point calculators (docs/STROKE.md,
/// "Width formulas"). `step` is the sample's travel in px.
fn node_for(kind: PenKind, cfg: &PenConfig, s: Sample, step: f32) -> Node {
    let p = s.pressure.clamp(0.0, 1.0);
    let w = cfg.width;
    let (size, alpha) = match kind {
        // BallpointPenCalculator: no pressure, no speed.
        PenKind::Ballpoint => (w / 2.0, 1.0),
        PenKind::Marker => unreachable!("the marker has its own pipeline (crate::marker)"),
        // SquarePen without directionEnabled: FixedWidthCalculator; size is the nib length, half the width (oracle).
        PenKind::Square => (w / 2.0, 1.0),
        // FountainPenV2 (tilt off, as knote2 configures it): FountainPenWidthCalculator.
        PenKind::Fountain => (fountain_width(cfg, p, step) / 2.0, 1.0),
        PenKind::Brush => unreachable!("the brush has its own pipeline (crate::brush)"),
        // PencilPen's PointCalculator: the fountain width as dab size. It also emits 0.6 · p, but
        // how the brush stage turns that into opacity is not decoded; as alpha it renders far
        // too faint, so the earlier openink opacity curve stays.
        PenKind::Pencil => (fountain_width(cfg, p, step), (0.35 + 0.8 * p.powf(1.1)).min(1.0)),
        // CharcoalPenV2's PointCalculator: fixed dab size, pressure as alpha, stepped tilt factor.
        PenKind::Charcoal => ((w * 1.2 + 5.0) * charcoal_tilt(cfg, s), p),
    };
    let floor = if kind.output() == Output::Dabs { 2.0 * MIN_HALF_PX } else { MIN_HALF_PX };
    Node { x: s.x, y: s.y, size: size.max(floor), alpha, src: s, step }
}

/// libneo's FountainPenWidthCalculator (diameter, px) with velocityAmplifier and the velocity
/// bounds at knote2's defaults (0, so inactive): `max(W·p^(2·sensitivity), minWidth)`, thinned by
/// speed `1 − 0.6·sensitivity·atan(0.06·v)` (v = travel per sample at 160 dpi), kept in [minWidth, W].
fn fountain_width(cfg: &PenConfig, p: f32, step: f32) -> f32 {
    let by_pressure = (cfg.width * p.powf(2.0 * cfg.pressure_sensitivity)).max(cfg.min_width);
    let thin = 1.0 - 0.6 * cfg.velocity_sensitivity * (0.06 * to_160dpi(cfg, step)).atan();
    (by_pressure * thin).clamp(cfg.min_width.min(cfg.width), cfg.width)
}

/// CharcoalPenV2: tilt θ from vertical (rad) scales the dab by `⌊(3·θ^2.2 + 1)·5⌋ / 5`.
fn charcoal_tilt(cfg: &PenConfig, s: Sample) -> f32 {
    if !cfg.tilt_enabled {
        return 1.0;
    }
    // libneo's angle from vertical (`FUN_004700a4`): acos(cos tiltX · cos tiltY), tilts in whole degrees.
    let theta = ((s.tilt_x as i32 as f32 * DEG).cos() * (s.tilt_y as i32 as f32 * DEG).cos()).clamp(-1.0, 1.0).acos();
    ((theta.powf(2.2) * 3.0 + 1.0) * 5.0).floor() / 5.0
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Catmull-Rom from `p1` to `p2` with `p0` before and the chord as the end tangent.
/// The segment p1 → p2 of a uniform Catmull-Rom spline over every sample channel, in libneo's matrix form
/// `[1, t, t², t³] · M · [p0, p1, p2, p3]` (SplineProducer; M from `_INIT_3`).
fn catmull_rom(p: &[Sample; 4], t: f32) -> Sample {
    let (t2, t3) = (t * t, t * t * t);
    let h = |a: f32, b: f32, c: f32, d: f32| {
        b + 0.5 * (c - a) * t + (a - 2.5 * b + 2.0 * c - 0.5 * d) * t2 + (-0.5 * a + 1.5 * b - 1.5 * c + 0.5 * d) * t3
    };
    let ch = |f: fn(&Sample) -> f32| h(f(&p[0]), f(&p[1]), f(&p[2]), f(&p[3]));
    Sample {
        x: ch(|s| s.x),
        y: ch(|s| s.y),
        pressure: ch(|s| s.pressure),
        tilt_x: ch(|s| s.tilt_x),
        tilt_y: ch(|s| s.tilt_y),
        t_ms: lerp(p[1].t_ms, p[2].t_ms, t),
    }
}

/// One channel of the uniform Catmull-Rom segment (same basis as [`catmull_rom`]).
fn cr1(p: [f32; 4], t: f32) -> f32 {
    let [a, b, c, d] = p;
    b + 0.5 * (c - a) * t + (a - 2.5 * b + 2.0 * c - 0.5 * d) * t * t + (-0.5 * a + 1.5 * b - 1.5 * c + 0.5 * d) * t * t * t
}

/// libneo's SplineCurvatureInterpolator on x/y (`FUN_00449c44`): split [t0, t1] at its middle while the curve's
/// midpoint lies more than `SPLIT_TOL` px from the chord (squared distance to the clamped projection), or more
/// than 10 · `SPLIT_TOL` from the chord's midpoint in x or y; the split points come out in order.
/// 0.15 / clamp(scalePrecision / 2, 1, 16) in libneo; scalePrecision is 1 for knote2 and SF (checked on the oracle).
const SPLIT_TOL: f32 = 0.15;
const SPLIT_DEPTH: u32 = 12;

fn subdivide(at: &impl Fn(f32) -> Sample, width: &impl Fn(f32) -> f32, t0: f32, t1: f32, out: &mut Vec<f32>, depth: u32) {
    let (a, b, tm) = (at(t0), at(t1), 0.5 * (t0 + t1));
    let m = at(tm);
    let (vx, vy) = (b.x - a.x, b.y - a.y);
    let l2 = vx * vx + vy * vy;
    let s = if l2 > 0.0 { (((m.x - a.x) * vx + (m.y - a.y) * vy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qy) = (a.x + vx * s, a.y + vy * s);
    let far = (m.x - qx).powi(2) + (m.y - qy).powi(2) > SPLIT_TOL * SPLIT_TOL
        || (m.x - 0.5 * (a.x + b.x)).abs() > 10.0 * SPLIT_TOL
        || (m.y - 0.5 * (a.y + b.y)).abs() > 10.0 * SPLIT_TOL
        // the width channel (`FUN_0044abd4`): its midpoint off the average of its ends by more than the tolerance
        || (width(tm) - 0.5 * (width(t0) + width(t1))).abs() > SPLIT_TOL;
    if far && depth < SPLIT_DEPTH {
        subdivide(at, width, t0, tm, out, depth + 1);
        out.push(tm);
        subdivide(at, width, tm, t1, out, depth + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: usize, pressure: impl Fn(f32) -> f32, ms_per_px: f32) -> Vec<Sample> {
        (0..n).map(|i| {
            let x = i as f32;
            Sample { x: 100.0 + x, y: 200.0, pressure: pressure(i as f32 / n as f32), tilt_x: 0.0, tilt_y: 0.0, t_ms: x * ms_per_px }
        }).collect()
    }

    fn cfg(kind: PenKind) -> PenConfig {
        PenConfig::knote2_default(kind, 300.0)
    }

    #[test]
    fn every_pen_renders_inside_its_bounds() {
        for kind in PenKind::ALL {
            let ink = Stroke::new(kind, cfg(kind)).render(&line(200, |t| 0.2 + 0.6 * t, 2.0));
            let [l, t, r, b] = ink.bounds.expect("non-empty");
            assert!(l < 101.0 && r > 298.0 && t < 200.0 && b > 200.0, "{kind:?} bounds {:?}", ink.bounds);
            assert!(ink.data.iter().all(|v| v.is_finite()), "{kind:?} non-finite");
            match ink.output {
                Output::Dabs => assert_eq!(ink.data.len() % 5, 0),
                _ => for poly in geom::polygons(&ink.data) {
                    assert!(geom::signed_area(poly) > 0.0, "{kind:?} polygon orientation");
                    for xy in poly.chunks(2) {
                        assert!(xy[0] >= l && xy[0] <= r && xy[1] >= t && xy[1] <= b, "{kind:?} point outside bounds");
                    }
                },
            }
        }
    }

    /// Values computed by hand from the libneo_pen.so decompile (docs/STROKE.md, "Width formulas").
    #[test]
    fn widths_match_libneo() {
        let cfg = PenConfig { width: 10.0, pressure_sensitivity: 0.3, smooth_level: 0.6, velocity_sensitivity: 0.5, tilt_enabled: true, tilt_scale: 3.0, dpi: 300.0, min_width: 0.001 };
        let at = |kind, p: f32, tilt: f32, step| node_for(kind, &cfg, Sample { x: 0.0, y: 0.0, pressure: p, tilt_x: tilt, tilt_y: 0.0, t_ms: 0.0 }, step);
        let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
        // Fountain: 10 · 0.39^0.6 = 5.684, halved; 30 px/sample = 16 at 160 dpi → × (1 − 0.3·atan 0.96).
        assert!(close(at(PenKind::Fountain, 0.39, 0.0, 0.0).size, 2.842));
        assert!(close(at(PenKind::Fountain, 0.39, 0.0, 30.0).size, 2.842 * (1.0 - 0.3 * 0.96f32.atan())));
        // Constant pens ignore pressure and speed; the square nib is half the width long (oracle).
        assert!(close(at(PenKind::Ballpoint, 0.1, 0.0, 30.0).size, 5.0) && close(at(PenKind::Square, 0.1, 0.0, 0.0).size, 5.0));
        // Pencil: the fountain width as the dab.
        assert!(close(at(PenKind::Pencil, 0.39, 0.0, 0.0).size, 5.684));
        // Charcoal: 1.2 · 10 + 5 = 17; at 45° the factor is ⌊(3 · 0.7854^2.2 + 1) · 5⌋ / 5 = 2.6.
        assert!(close(at(PenKind::Charcoal, 0.5, 0.0, 0.0).size, 17.0) && close(at(PenKind::Charcoal, 0.5, 0.0, 0.0).alpha, 0.5));
        assert!(close(at(PenKind::Charcoal, 0.5, 45.0, 0.0).size, 17.0 * 2.6));
    }

    #[test]
    fn a_tap_leaves_a_mark() {
        for kind in PenKind::ALL {
            let s = Sample { x: 10.0, y: 10.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0.0 };
            assert!(!Stroke::new(kind, cfg(kind)).render(&[s]).is_empty(), "{kind:?}");
        }
    }

    #[test]
    fn fountain_follows_pressure_ballpoint_does_not() {
        let half = |kind, p, speed| node_for(kind, &cfg(kind), Sample { x: 0.0, y: 0.0, pressure: p, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0.0 }, speed).size;
        let ratio = |kind| half(kind, 0.1, 0.0) / half(kind, 1.0, 0.0);
        assert!(ratio(PenKind::Fountain) < 0.6);
        assert!(ratio(PenKind::Ballpoint) > 0.8);
        assert!(half(PenKind::Fountain, 0.8, 5.0) < half(PenKind::Fountain, 0.8, 0.0), "speed thins");
    }

    #[test]
    fn incremental_equals_whole() {
        let pts = line(120, |t| 0.5 + 0.3 * t, 1.0);
        for kind in PenKind::ALL {
            let whole = Stroke::new(kind, cfg(kind)).render(&pts);
            let (mut a, mut ink) = Stroke::new(kind, cfg(kind)).down(pts[0]);
            for chunk in pts[1..pts.len() - 1].chunks(7) {
                ink.extend(a.moved(chunk));
            }
            ink.extend(a.up(pts[pts.len() - 1]));
            assert_eq!(ink, whole, "{kind:?}");
        }
    }

    #[test]
    fn pencil_dabs_keep_libneos_spacing() {
        // libneo: a dab once it is ≥ 0.25 · the mean dab size from the last one; candidates are the samples here
        // (2 px apart, n = 1), so no gap is shorter than the spacing nor longer than spacing + one sample.
        let ink = Stroke::new(PenKind::Pencil, cfg(PenKind::Pencil)).render(&line(300, |_| 0.6, 2.0));
        let dabs: Vec<&[f32]> = ink.data.chunks(5).collect();
        for w in dabs.windows(2) {
            let (gap, need) = (w[1][0] - w[0][0], (w[0][2] + w[1][2]) * 0.5 * PENCIL_SPACING);
            assert!(gap >= need - 1e-3 && gap <= need + 2.0 + 1e-3, "gap {gap} vs spacing {need}");
        }
    }

    /// libneo's angles for the user's stroke s2 (first dab p 0.3814 → seed 13·390): 220°, 251°, 339°, 6°, 20°.
    #[test]
    fn pencil_angles_follow_java_random() {
        let mut r = DistanceDabs::new(PENCIL_SPACING, Sample { x: 0.0, y: 0.0, pressure: 0.3814, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0.0 }).rng;
        assert_eq!([0; 5].map(|_| r.next_int(360)), [220, 251, 339, 6, 20]);
    }

    #[test]
    fn non_finite_samples_are_skipped() {
        let mut pts = line(60, |_| 0.5, 1.0);
        pts[0].y = f32::NAN;
        pts[20].x = f32::NAN;
        pts[30].pressure = f32::INFINITY;
        for kind in PenKind::ALL {
            let ink = Stroke::new(kind, cfg(kind)).render(&pts);
            assert!(!ink.is_empty() && ink.data.iter().all(|v| v.is_finite()), "{kind:?}");
        }
    }

    #[test]
    fn config_is_sanitized() {
        let bad = PenConfig { width: f32::NAN, pressure_sensitivity: 7.0, smooth_level: -1.0, velocity_sensitivity: f32::INFINITY, tilt_enabled: true, tilt_scale: 0.0, dpi: f32::NAN, min_width: -3.0 };
        let s = Stroke::new(PenKind::Fountain, bad).cfg;
        assert!(s.width == 3.0 && s.pressure_sensitivity == 1.0 && s.smooth_level == 0.0 && s.velocity_sensitivity == 0.0 && s.tilt_scale == 1.0);
    }
}

//! Inference for the two trained models (training: docs/scripts/train_shape.py):
//! - `Classifier`: an MLP over the per-ink features → family probabilities;
//! - `Segmenter`: a bidirectional GRU over the per-point features → straight / curve / corner.
//!
//! Weight file format and the layers themselves: the `openink-nn` crate.

pub use openink_nn::WeightsError;
use openink_nn::{softmax, BiGru, Dense, Gru, GruLayout, Scaler, Store};

/// MLP: tanh hidden layers, softmax output.
#[derive(Clone, Debug)]
pub struct Classifier {
    scaler: Scaler,
    layers: Vec<Dense>,
}

impl Classifier {
    pub fn probabilities(&self, features: &[f32]) -> Vec<f32> {
        let mut x = self.scaler.apply(features);
        for (i, l) in self.layers.iter().enumerate() {
            let mut y = vec![0.0; l.out];
            l.apply(&x, &mut y);
            if i + 1 < self.layers.len() {
                y.iter_mut().for_each(|v| *v = v.tanh());
            }
            x = y;
        }
        softmax(&mut x);
        x
    }
}

/// Per-point labels from the segmenter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointLabel {
    Straight = 0,
    Curve = 1,
    Corner = 2,
}

pub const POINT_LABELS: usize = 3;

#[derive(Clone, Debug)]
pub struct Segmenter {
    scaler: Scaler,
    gru: BiGru,
    out: Dense,
}

impl Segmenter {
    /// Probabilities `[straight, curve, corner]` per point.
    pub fn probabilities(&self, features: &[[f32; crate::features::POINT_FEATURES]]) -> Vec<[f32; POINT_LABELS]> {
        let xs: Vec<Vec<f32>> = features.iter().map(|f| self.scaler.apply(f)).collect();
        self.gru
            .run(&xs)
            .iter()
            .map(|cat| {
                let mut y = [0.0f32; POINT_LABELS];
                self.out.apply(cat, &mut y);
                softmax(&mut y);
                y
            })
            .collect()
    }
}

/// Both trained models, loaded from one `.oisw` file.
#[derive(Clone, Debug)]
pub struct Models {
    pub classifier: Classifier,
    pub segmenter: Segmenter,
}

impl Models {
    pub fn from_bytes(bytes: &[u8]) -> Result<Models, WeightsError> {
        use crate::features::{POINT_FEATURES, STROKE_FEATURES};
        let s = Store::from_bytes(bytes)?;
        let mut layers = Vec::new();
        while s.contains(&format!("clf.l{}.w", layers.len())) {
            layers.push(Dense::load(&s, &format!("clf.l{}", layers.len()))?);
        }
        let shape_ok = layers.first().is_some_and(|l| l.inp == STROKE_FEATURES)
            && layers.last().is_some_and(|l| l.out == crate::model::Family::COUNT)
            && layers.windows(2).all(|w| w[0].out == w[1].inp);
        if !shape_ok {
            return Err(WeightsError::Shape("clf".into()));
        }
        let classifier = Classifier { scaler: Scaler::load(&s, "clf", STROKE_FEATURES)?, layers };
        let gru = BiGru {
            fwd: Gru::load(&s, "seg.f", POINT_FEATURES, GruLayout::Zrn)?,
            bwd: Gru::load(&s, "seg.b", POINT_FEATURES, GruLayout::Zrn)?,
        };
        let out = Dense::load(&s, "seg.out")?;
        if out.inp != gru.out_dim() || out.out != POINT_LABELS {
            return Err(WeightsError::Shape("seg.out".into()));
        }
        let segmenter = Segmenter { scaler: Scaler::load(&s, "seg", POINT_FEATURES)?, gru, out };
        Ok(Models { classifier, segmenter })
    }
}

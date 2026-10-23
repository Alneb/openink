//! Open online handwriting recogniser. A word's pen strokes → per-point features → a stack of
//! bidirectional GRUs → per-point character log-probabilities → CTC decoding to text candidates.
//! Training: docs/scripts/train_hwr.py (synthetic Hershey ink + the user's labelled notes).
//! Design, data and measured accuracy: docs/HWR-ENGINE.md.

pub mod ctc;
pub mod lm;
pub mod features;

pub use features::{features, slant, Point, POINT_FEATURES};
use openink_nn::{log_softmax, BiGru, Dense, Gru, GruLayout, Scaler, Store, WeightsError};

/// One reading of the ink, best first in [`Recognizer::recognize`]'s output.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub text: String,
    /// Natural-log probability of the text under the model (CTC prefix score).
    pub log_prob: f32,
}

/// The trained network plus its output alphabet (class k + 1 is `alphabet[k]`; class 0 is the CTC blank).
#[derive(Clone, Debug)]
pub struct Recognizer {
    scaler: Scaler,
    layers: Vec<BiGru>,
    out: Dense,
    alphabet: Vec<char>,
    lexicon: Option<ctc::Trie>,
    lm: Option<lm::CharLm>,
    /// Features are taken after slant correction (`hwr.deslant` = 1 in the weight file).
    deslant: bool,
    /// Beam width for [`Recognizer::recognize`].
    pub beam: usize,
    /// Log-probability bonus a dictionary word gets over an open-vocabulary reading.
    pub lexicon_bonus: f32,
    /// Weight of the character LM's log-probability in the ranking (with [`Recognizer::set_language_model`]).
    pub lm_weight: f32,
    /// Per-character bonus that balances the LM's cost per character.
    pub insertion_bonus: f32,
}

impl Recognizer {
    /// Tensors `hwr.deslant` (optional, [1]), `hwr.mu/sd`, `hwr.l{k}.{f,b}.{w,u,b,c}` (torch GRU layout), `hwr.out.{w,b}`, and
    /// `hwr.alphabet` (Unicode scalar values stored as f32, exact below 2^24).
    pub fn from_bytes(bytes: &[u8]) -> Result<Recognizer, WeightsError> {
        let s = Store::from_bytes(bytes)?;
        let mut layers = Vec::new();
        let mut inp = POINT_FEATURES;
        while s.contains(&format!("hwr.l{}.f.w", layers.len())) {
            let k = layers.len();
            let l = BiGru {
                fwd: Gru::load(&s, &format!("hwr.l{k}.f"), inp, GruLayout::Torch)?,
                bwd: Gru::load(&s, &format!("hwr.l{k}.b"), inp, GruLayout::Torch)?,
            };
            inp = l.out_dim();
            layers.push(l);
        }
        if layers.is_empty() {
            return Err(WeightsError::Missing("hwr.l0.f.w".into()));
        }
        let out = Dense::load(&s, "hwr.out")?;
        let n = s.dims("hwr.alphabet")?;
        let alphabet: Vec<char> = s
            .get("hwr.alphabet", &n)?
            .iter()
            .map(|&c| char::from_u32(c as u32).ok_or_else(|| WeightsError::Shape("hwr.alphabet".into())))
            .collect::<Result<_, _>>()?;
        // Absent in files from before slant correction existed: those models read raw ink.
        let deslant = s.contains("hwr.deslant") && s.get("hwr.deslant", &[1])?[0] != 0.0;
        if out.inp != inp || out.out != alphabet.len() + 1 {
            return Err(WeightsError::Shape("hwr.out".into()));
        }
        Ok(Recognizer { scaler: Scaler::load(&s, "hwr", POINT_FEATURES)?, layers, out, alphabet, lexicon: None, lm: None, deslant, beam: 8, lexicon_bonus: 4.0, lm_weight: 0.2, insertion_bonus: 0.5 })
    }

    /// The model shipped in weights/hwr.oisw (trained by docs/scripts/train_hwr.py).
    pub fn builtin() -> Recognizer {
        Recognizer::from_bytes(include_bytes!("../weights/hwr.oisw")).expect("built-in weights/hwr.oisw is valid")
    }

    /// Adds a dictionary: each word as given and with its first letter capitalised. Words using
    /// characters outside the alphabet are skipped. Returns how many entries the trie holds.
    pub fn set_lexicon<'a>(&mut self, words: impl IntoIterator<Item = &'a str>) -> usize {
        let mut t = ctc::Trie::new();
        for w in words {
            let mut cap = w.chars();
            let capitalised: String = cap.next().map(|c| c.to_uppercase().chain(cap).collect()).unwrap_or_default();
            for v in [w, capitalised.as_str()] {
                if let Some(ids) = self.ids_of(v) {
                    if !ids.is_empty() {
                        t.insert(&ids);
                    }
                }
            }
        }
        let n = t.len();
        self.lexicon = (n > 0).then_some(t);
        n
    }

    /// Trains a character `order`-gram LM on `texts` (e.g. a word list plus the user's own words) and
    /// fuses it into [`Recognizer::recognize`]'s ranking. Texts with characters outside the alphabet are
    /// skipped. Returns how many texts were used.
    pub fn set_language_model<'a>(&mut self, texts: impl IntoIterator<Item = &'a str>, order: usize) -> usize {
        let seqs: Vec<Vec<usize>> = texts.into_iter().filter_map(|t| self.ids_of(t)).filter(|v| !v.is_empty()).collect();
        self.lm = (!seqs.is_empty()).then(|| lm::CharLm::train(order, self.alphabet.len() + 1, seqs.iter().map(|v| v.as_slice())));
        seqs.len()
    }

    fn ids_of(&self, text: &str) -> Option<Vec<usize>> {
        text.chars().map(|c| self.class_of(c)).collect()
    }

    fn class_of(&self, c: char) -> Option<usize> {
        self.alphabet.iter().position(|&a| a == c).map(|k| k + 1)
    }

    fn text(&self, ids: &[usize]) -> String {
        ids.iter().map(|&k| self.alphabet[k - 1]).collect()
    }

    pub fn alphabet(&self) -> &[char] {
        &self.alphabet
    }

    /// Per-point log-probabilities over `[blank, alphabet...]`.
    pub fn log_probs(&self, strokes: &[Vec<Point>]) -> Vec<Vec<f32>> {
        let mut x: Vec<Vec<f32>> = features(strokes, self.deslant).iter().map(|f| self.scaler.apply(f)).collect();
        for l in &self.layers {
            x = l.run(&x);
        }
        x.iter()
            .map(|h| {
                let mut y = vec![0.0; self.out.out];
                self.out.apply(h, &mut y);
                log_softmax(&mut y);
                y
            })
            .collect()
    }

    /// Up to `beam` readings of one word, best first. Ranked by the CTC log-probability, plus the
    /// fused character LM when one is set; with a lexicon, dictionary words found by a trie-constrained
    /// search compete at `+ lexicon_bonus`. The reported `log_prob` stays the model's own. Empty ink gives
    /// no candidates.
    pub fn recognize(&self, strokes: &[Vec<Point>]) -> Vec<Candidate> {
        let lp = self.log_probs(strokes);
        let fusion = self.lm.as_ref().map(|lm| ctc::Fusion { lm, weight: self.lm_weight, insertion: self.insertion_bonus });
        let mut scored: Vec<(f32, Candidate)> = ctc::beam_search_lm(&lp, self.beam, fusion)
            .into_iter()
            .map(|h| (h.score, Candidate { text: self.text(&h.ids), log_prob: h.log_prob }))
            .collect();
        if let Some(t) = &self.lexicon {
            for (ids, log_prob) in ctc::lexicon_search(&lp, t, self.beam) {
                let lm = fusion.map_or(0.0, |f| f.weight * f.lm.score(&ids) + f.insertion * ids.len() as f32);
                scored.push((log_prob + lm + self.lexicon_bonus, Candidate { text: self.text(&ids), log_prob }));
            }
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut seen = std::collections::HashSet::new();
        scored.into_iter().map(|(_, c)| c).filter(|c| seen.insert(c.text.clone())).take(self.beam).collect()
    }

    /// The single most likely reading by best-path decoding (fast; what the trainer reports).
    pub fn greedy(&self, strokes: &[Vec<Point>]) -> String {
        self.text(&ctc::greedy(&self.log_probs(strokes)))
    }
}

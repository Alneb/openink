//! CTC decoding over per-step log-probabilities (class 0 = blank).

use std::collections::HashMap;

use crate::lm::CharLm;

/// Best path: argmax per step, merge repeats, drop blanks.
pub fn greedy(lp: &[Vec<f32>]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut prev = 0;
    for row in lp {
        let k = (0..row.len()).max_by(|&a, &b| row[a].total_cmp(&row[b])).unwrap_or(0);
        if k != prev && k != 0 {
            out.push(k);
        }
        prev = k;
    }
    out
}

fn lse(a: f32, b: f32) -> f32 {
    if a == f32::NEG_INFINITY {
        return b;
    }
    if b == f32::NEG_INFINITY {
        return a;
    }
    let m = a.max(b);
    m + ((a - m).exp() + (b - m).exp()).ln()
}

/// Prefix beam search: each prefix keeps the log-probability of ending in a blank (`b`) or in its
/// last label (`nb`), so "aa" (needs a blank between) and "a" (a held label) stay distinct.
/// Returns up to `width` prefixes with their total log-probability, best first.
pub fn beam_search(lp: &[Vec<f32>], width: usize) -> Vec<(Vec<usize>, f32)> {
    beam_search_lm(lp, width, None).into_iter().map(|h| (h.ids, h.log_prob)).collect()
}

/// A language model folded into the beam ranking (shallow fusion):
/// rank = log P_ctc + `weight` · log P_lm + `insertion` · characters.
#[derive(Clone, Copy, Debug)]
pub struct Fusion<'a> {
    pub lm: &'a CharLm,
    pub weight: f32,
    /// Offsets the LM's per-character cost, which otherwise favours dropping characters.
    pub insertion: f32,
}

/// One beam result.
#[derive(Clone, Debug, PartialEq)]
pub struct Hyp {
    pub ids: Vec<usize>,
    /// CTC log-probability alone.
    pub log_prob: f32,
    /// The ranking score: equal to `log_prob` without fusion; with it, includes the word's end.
    pub score: f32,
}

/// [`beam_search`], optionally with an LM in the ranking. Best `score` first.
pub fn beam_search_lm(lp: &[Vec<f32>], width: usize, fusion: Option<Fusion>) -> Vec<Hyp> {
    const NEG: f32 = f32::NEG_INFINITY;
    let width = width.max(1);
    let lm_step = |prefix: &[usize], k: usize| fusion.map_or(0.0, |f| f.weight * f.lm.log_prob(prefix, k) + f.insertion);
    // (prefix, log p ending in blank, log p ending in label, accumulated LM term)
    let mut beams: Vec<(Vec<usize>, f32, f32, f32)> = vec![(Vec::new(), 0.0, NEG, 0.0)];
    for row in lp {
        // Only labels that could matter at this step: the top few by probability.
        let mut cand: Vec<usize> = (1..row.len()).collect();
        cand.sort_by(|&a, &b| row[b].total_cmp(&row[a]));
        cand.truncate(width + 4);
        let mut next: HashMap<Vec<usize>, (f32, f32, f32)> = HashMap::new();
        for (prefix, b, nb, lm) in &beams {
            let total = lse(*b, *nb);
            let e = next.entry(prefix.clone()).or_insert((NEG, NEG, *lm));
            e.0 = lse(e.0, total + row[0]); // blank
            if let Some(&last) = prefix.last() {
                e.1 = lse(e.1, nb + row[last]); // held label, no new character
            }
            for &k in &cand {
                let mut p = prefix.clone();
                p.push(k);
                // Repeating the last label only starts a new character after a blank.
                let from = if prefix.last() == Some(&k) { *b } else { total };
                let e = next.entry(p).or_insert_with(|| (NEG, NEG, lm + lm_step(prefix, k)));
                e.1 = lse(e.1, from + row[k]);
            }
        }
        let mut v: Vec<(Vec<usize>, f32, f32, f32)> = next.into_iter().map(|(p, (b, nb, lm))| (p, b, nb, lm)).collect();
        v.sort_by(|x, y| (lse(y.1, y.2) + y.3).total_cmp(&(lse(x.1, x.2) + x.3)).then_with(|| x.0.cmp(&y.0)));
        v.truncate(width);
        beams = v;
    }
    let mut out: Vec<Hyp> = beams
        .into_iter()
        .filter(|(p, ..)| !p.is_empty())
        .map(|(ids, b, nb, lm)| {
            let log_prob = lse(b, nb);
            let end = fusion.map_or(0.0, |f| f.weight * f.lm.log_prob(&ids, 0));
            Hyp { score: log_prob + lm + end, log_prob, ids }
        })
        .collect();
    out.sort_by(|x, y| y.score.total_cmp(&x.score));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(ps: &[&[f32]]) -> Vec<Vec<f32>> {
        ps.iter().map(|r| r.iter().map(|p| p.ln()).collect()).collect()
    }

    #[test]
    fn greedy_merges_repeats_and_blanks() {
        let lp = rows(&[&[0.1, 0.8, 0.1], &[0.1, 0.8, 0.1], &[0.8, 0.1, 0.1], &[0.1, 0.8, 0.1], &[0.1, 0.1, 0.8]]);
        assert_eq!(greedy(&lp), vec![1, 1, 2]);
    }

    /// The classic case where best path and best labelling disagree: summed over paths, "1" beats "".
    #[test]
    fn beam_sums_paths() {
        let lp = rows(&[&[0.6, 0.4], &[0.6, 0.4]]);
        assert!(greedy(&lp).is_empty());
        let best = &beam_search(&lp, 4)[0];
        assert_eq!(best.0, vec![1]);
        // P("1") = 0.4*0.4 + 0.4*0.6 + 0.6*0.4 = 0.64
        assert!((best.1.exp() - 0.64).abs() < 1e-5);
    }

    /// A close acoustic call between classes 1 and 2 goes to the one the LM has seen.
    #[test]
    fn fusion_breaks_a_close_call() {
        let lp = rows(&[&[0.1, 0.44, 0.46]]);
        assert_eq!(beam_search(&lp, 4)[0].0, vec![2]);
        let words: Vec<Vec<usize>> = vec![vec![1]; 5];
        let lm = CharLm::train(2, 3, words.iter().map(|w| w.as_slice()));
        let hyps = beam_search_lm(&lp, 4, Some(Fusion { lm: &lm, weight: 0.5, insertion: 0.0 }));
        assert_eq!(hyps[0].ids, vec![1]);
        assert!((hyps[0].log_prob - 0.44f32.ln()).abs() < 1e-5);
    }
}

/// Words as a trie over class ids, for [`lexicon_search`].
#[derive(Clone, Debug, Default)]
pub struct Trie {
    next: Vec<HashMap<usize, usize>>,
    word: Vec<bool>,
}

impl Trie {
    pub fn new() -> Trie {
        Trie { next: vec![HashMap::new()], word: vec![false] }
    }

    pub fn insert(&mut self, ids: &[usize]) {
        let mut n = 0;
        for &k in ids {
            n = match self.next[n].get(&k) {
                Some(&m) => m,
                None => {
                    self.next.push(HashMap::new());
                    self.word.push(false);
                    let m = self.next.len() - 1;
                    self.next[n].insert(k, m);
                    m
                }
            };
        }
        self.word[n] = true;
    }

    pub fn len(&self) -> usize {
        self.word.iter().filter(|w| **w).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Prefix beam search restricted to paths in `trie`; returns complete words only, best first.
pub fn lexicon_search(lp: &[Vec<f32>], trie: &Trie, width: usize) -> Vec<(Vec<usize>, f32)> {
    const NEG: f32 = f32::NEG_INFINITY;
    let width = width.max(1);
    // (prefix, trie node, log p ending in blank, log p ending in label)
    let mut beams: Vec<(Vec<usize>, usize, f32, f32)> = vec![(Vec::new(), 0, 0.0, NEG)];
    for row in lp {
        let mut next: HashMap<Vec<usize>, (usize, f32, f32)> = HashMap::new();
        for (prefix, node, b, nb) in &beams {
            let total = lse(*b, *nb);
            let e = next.entry(prefix.clone()).or_insert((*node, NEG, NEG));
            e.1 = lse(e.1, total + row[0]);
            if let Some(&last) = prefix.last() {
                e.2 = lse(e.2, nb + row[last]);
            }
            for (&k, &child) in &trie.next[*node] {
                let mut p = prefix.clone();
                p.push(k);
                let from = if prefix.last() == Some(&k) { *b } else { total };
                let e = next.entry(p).or_insert((child, NEG, NEG));
                e.2 = lse(e.2, from + row[k]);
            }
        }
        let mut v: Vec<(Vec<usize>, usize, f32, f32)> = next.into_iter().map(|(p, (n, b, nb))| (p, n, b, nb)).collect();
        v.sort_by(|x, y| lse(y.2, y.3).total_cmp(&lse(x.2, x.3)).then_with(|| x.0.cmp(&y.0)));
        v.truncate(width);
        beams = v;
    }
    beams.into_iter().filter(|(p, n, _, _)| !p.is_empty() && trie.word[*n]).map(|(p, _, b, nb)| (p, lse(b, nb))).collect()
}

#[cfg(test)]
mod lexicon_tests {
    use super::*;

    #[test]
    fn lexicon_overrides_a_close_call() {
        // Step 2 slightly prefers class 2, but only [1, 1] ("aa" needs a blank) and [1, 3] are words.
        let lp: Vec<Vec<f32>> = [[0.1, 0.8, 0.05, 0.05], [0.1, 0.05, 0.45, 0.4]]
            .iter()
            .map(|r| r.iter().map(|p: &f32| p.ln()).collect())
            .collect();
        let mut t = Trie::new();
        t.insert(&[1, 3]);
        t.insert(&[1, 1]);
        assert_eq!(beam_search(&lp, 4)[0].0, vec![1, 2]);
        let best = &lexicon_search(&lp, &t, 8)[0];
        assert_eq!(best.0, vec![1, 3]);
        assert!((best.1.exp() - 0.8 * 0.4).abs() < 1e-5);
        assert_eq!(t.len(), 2);
    }
}

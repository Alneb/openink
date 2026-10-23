//! Character n-gram language model over class ids, for shallow fusion in the CTC beam
//! ([`crate::ctc::beam_search_lm`]). Interpolated Witten–Bell smoothing, built at run time from a word
//! list, so no LM file ships. Class 0 (the CTC blank, never a character) marks the word boundary.

use std::collections::HashMap;

const BOUNDARY: usize = 0;

/// Follower counts of one context.
#[derive(Clone, Debug, Default)]
struct Ctx {
    next: HashMap<usize, u32>,
    total: u32,
}

#[derive(Clone, Debug)]
pub struct CharLm {
    order: usize,
    /// Number of symbols (alphabet + boundary): the uniform base distribution.
    vocab: usize,
    /// Context (the last 0..order-1 ids, oldest first) → follower counts.
    ctx: HashMap<Vec<usize>, Ctx>,
}

impl CharLm {
    /// `order` ≥ 1 (1 = unigram); `vocab` = classes including the boundary id 0. Each word is a
    /// sequence of class ids (no 0s).
    pub fn train<'a>(order: usize, vocab: usize, words: impl IntoIterator<Item = &'a [usize]>) -> CharLm {
        let order = order.max(1);
        let mut ctx: HashMap<Vec<usize>, Ctx> = HashMap::new();
        for w in words {
            let mut seq = vec![BOUNDARY; order - 1];
            seq.extend_from_slice(w);
            seq.push(BOUNDARY);
            for i in order - 1..seq.len() {
                for n in 0..order {
                    let c = ctx.entry(seq[i - n..i].to_vec()).or_default();
                    *c.next.entry(seq[i]).or_insert(0) += 1;
                    c.total += 1;
                }
            }
        }
        CharLm { order, vocab: vocab.max(1), ctx }
    }

    /// log P(`next` | the word so far `history`); `next` = 0 asks for the end of the word.
    pub fn log_prob(&self, history: &[usize], next: usize) -> f32 {
        let mut h = vec![BOUNDARY; (self.order - 1).saturating_sub(history.len())];
        h.extend_from_slice(&history[history.len().saturating_sub(self.order - 1)..]);
        // Shortest context first: P_n = (c(h,w) + T(h)·P_{n-1}) / (c(h) + T(h)).
        let mut p = 1.0 / self.vocab as f64;
        for n in 0..self.order {
            if let Some(c) = self.ctx.get(&h[h.len() - n..]) {
                let t = c.next.len() as f64;
                let cw = *c.next.get(&next).unwrap_or(&0) as f64;
                p = (cw + t * p) / (c.total as f64 + t);
            }
        }
        p.ln() as f32
    }

    /// log P of a whole word, including its end.
    pub fn score(&self, ids: &[usize]) -> f32 {
        (0..=ids.len()).map(|i| self.log_prob(&ids[..i], ids.get(i).copied().unwrap_or(BOUNDARY))).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distributions_sum_to_one() {
        let words: Vec<Vec<usize>> = vec![vec![1, 2, 3], vec![1, 2], vec![3, 1]];
        let lm = CharLm::train(3, 4, words.iter().map(|w| w.as_slice()));
        for h in [&[][..], &[1], &[1, 2], &[3, 3, 3]] {
            let s: f64 = (0..4).map(|k| (lm.log_prob(h, k) as f64).exp()).sum();
            assert!((s - 1.0).abs() < 1e-5, "history {h:?}: {s}");
        }
    }

    #[test]
    fn seen_words_beat_scrambles() {
        let words: Vec<Vec<usize>> = vec![vec![1, 2, 3], vec![1, 2, 4], vec![2, 3]];
        let lm = CharLm::train(3, 5, words.iter().map(|w| w.as_slice()));
        assert!(lm.score(&[1, 2, 3]) > lm.score(&[3, 2, 1]));
        assert!(lm.log_prob(&[1], 2) > lm.log_prob(&[1], 4));
    }
}

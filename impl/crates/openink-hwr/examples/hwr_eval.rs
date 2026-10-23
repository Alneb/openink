//! Scores the recogniser on the user's labelled test words (ids with id % 5 == 0, as in train_hwr.py)
//! under each decoding mode, and sweeps the LM fusion weights.
//!
//! cargo run --release -p openink-hwr --example hwr_eval -- WEIGHTS words-labelled.json labels-all.tsv \
//!     [WORDLIST]   (one word per line, e.g. /usr/share/dict/words; the train-split labels are added to it)

use std::collections::HashMap;

use openink_hwr::{Point, Recognizer};

fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != cb)).min(row[j] + 1).min(row[j + 1] + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

/// (CER, word accuracy) of `read` over the test words.
fn score(test: &[(Vec<Vec<Point>>, String)], read: impl Fn(&[Vec<Point>]) -> String) -> (f32, f32) {
    let (mut errs, mut chars, mut right) = (0, 0, 0);
    for (strokes, truth) in test {
        let got = read(strokes);
        let (g, t): (Vec<char>, Vec<char>) = (got.chars().collect(), truth.chars().collect());
        errs += edit_distance(&g, &t);
        chars += t.len();
        right += usize::from(got == *truth);
    }
    (errs as f32 / chars.max(1) as f32, right as f32 / test.len().max(1) as f32)
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 3 {
        eprintln!("usage: hwr_eval WEIGHTS words-labelled.json labels-all.tsv [WORDLIST]");
        std::process::exit(2);
    }
    let rec = Recognizer::from_bytes(&std::fs::read(&a[0]).expect("weights")).expect("valid weights");
    let words: HashMap<String, serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(&a[1]).expect("words json")).expect("json");
    let labels: Vec<(u64, String)> = std::fs::read_to_string(&a[2])
        .expect("labels")
        .lines()
        .filter_map(|l| {
            let (k, t) = l.split_once('\t')?;
            Some((k.parse().ok()?, t.to_string())).filter(|(_, t)| !t.is_empty())
        })
        .collect();
    let mut test = Vec::new();
    let mut vocab: Vec<String> = Vec::new();
    for (id, text) in &labels {
        if id % 5 != 0 {
            vocab.push(text.clone());
            continue;
        }
        let Some(w) = words.get(&id.to_string()) else { continue };
        let strokes: Vec<Vec<Point>> = w["strokes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_array().unwrap().iter().map(|p| [p[0].as_f64().unwrap() as f32, p[1].as_f64().unwrap() as f32]).collect())
            .collect();
        test.push((strokes, text.clone()));
    }
    // One or more word lists, comma-separated (e.g. a physics vocabulary and the system dictionary).
    for path in a.get(3).into_iter().flat_map(|p| p.split(',')) {
        vocab.extend(std::fs::read_to_string(path).expect("word list").lines().map(str::to_string));
    }
    println!("{} test words; vocabulary {} texts (train-split labels + word list)", test.len(), vocab.len());

    let best = |r: &Recognizer, s: &[Vec<Point>]| r.recognize(s).first().map(|c| c.text.clone()).unwrap_or_default();
    let (c, w) = score(&test, |s| rec.greedy(s));
    println!("greedy                    CER {c:.3}  word {w:.3}");
    let (c, w) = score(&test, |s| best(&rec, s));
    println!("beam {}                    CER {c:.3}  word {w:.3}", rec.beam);

    let mut lex = rec.clone();
    let n = lex.set_lexicon(vocab.iter().map(String::as_str));
    for bonus in [1.0, 2.0, 4.0] {
        lex.lexicon_bonus = bonus;
        let (c, w) = score(&test, |s| best(&lex, s));
        println!("+ lexicon ({n}) bonus {bonus}   CER {c:.3}  word {w:.3}");
    }

    let mut lm = rec.clone();
    let n = lm.set_language_model(vocab.iter().map(String::as_str), 4);
    println!("LM: 4-gram on {n} texts");
    for weight in [0.2, 0.4, 0.7] {
        for ins in [0.0, 0.5, 1.0] {
            lm.lm_weight = weight;
            lm.insertion_bonus = ins;
            let (c, w) = score(&test, |s| best(&lm, s));
            println!("+ LM weight {weight} ins {ins}      CER {c:.3}  word {w:.3}");
        }
    }
}

//! JNI side of `org.openink.hwr.HandwritingRecognizer` (crates/openink-hwr; replaces MyScript Text).
//! Input: the same `xyt` / `lens` arrays as the shape recogniser (t is ignored). Output: one string,
//! `log_prob \t text` per candidate, best first, lines joined by `\n` (the alphabet has neither character).

use crate::shape::read_ink;
use jni::objects::{JClass, JFloatArray, JIntArray};
use jni::sys::jstring;
use jni::JNIEnv;
use openink_hwr::{Candidate, Point, Recognizer};
use std::sync::OnceLock;

fn recognizer() -> &'static Recognizer {
    static R: OnceLock<Recognizer> = OnceLock::new();
    R.get_or_init(Recognizer::builtin)
}

pub fn encode(c: &[Candidate]) -> String {
    c.iter().map(|c| format!("{}\t{}", c.log_prob, c.text)).collect::<Vec<_>>().join("\n")
}

#[no_mangle]
pub extern "system" fn Java_org_openink_hwr_HandwritingRecognizer_nativeRecognize(
    mut env: JNIEnv,
    _c: JClass,
    xyt: JFloatArray,
    lens: JIntArray,
) -> jstring {
    let Some(strokes) = read_ink(&mut env, &xyt, &lens, "HandwritingRecognizer") else { return std::ptr::null_mut() };
    let pts: Vec<Vec<Point>> = strokes.iter().map(|s| s.iter().map(|p| [p.x, p.y]).collect()).collect();
    match env.new_string(encode(&recognizer().recognize(&pts))) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_lines() {
        let c = [Candidate { text: "ab c".into(), log_prob: -0.5 }, Candidate { text: "x".into(), log_prob: -2.0 }];
        assert_eq!(encode(&c), "-0.5\tab c\n-2\tx");
        assert_eq!(encode(&[]), "");
    }
}

//! JNI side of `org.openink.hwr.HandwritingRecognizer` (crates/openink-hwr; replaces MyScript Text).
//! Input: the same `xyt` / `lens` arrays as the shape recogniser (t is ignored). Output: one string,
//! `log_prob \t text` per candidate, best first, lines joined by `\n` (the alphabet has neither character).

use crate::shape::strokes_from;
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
    let read = |env: &mut JNIEnv| -> Option<(Vec<f32>, Vec<i32>)> {
        let mut a = vec![0f32; env.get_array_length(&xyt).ok()? as usize];
        env.get_float_array_region(&xyt, 0, &mut a).ok()?;
        let mut b = vec![0i32; env.get_array_length(&lens).ok()? as usize];
        env.get_int_array_region(&lens, 0, &mut b).ok()?;
        Some((a, b))
    };
    let Some(strokes) = read(&mut env).and_then(|(a, b)| strokes_from(&a, &b)) else {
        let _ = env.throw_new("java/lang/IllegalArgumentException", "HandwritingRecognizer: malformed ink arrays");
        return std::ptr::null_mut();
    };
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

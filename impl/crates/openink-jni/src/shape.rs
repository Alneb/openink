//! JNI side of `org.openink.shape.ShapeRecognizer` (crates/openink-shape; replaces MyScript Shape).
//! Input: `xyt float[3 n]` (x, y, t ms) for all strokes back to back, `lens int[]` samples per stroke.
//! Output: `openink_shape::wire::encode`, or null for malformed input (with an exception thrown).

use jni::objects::{JClass, JFloatArray, JIntArray};
use jni::sys::jfloatArray;
use jni::JNIEnv;

use crate::jvm;
use openink_shape::features::Sample;
use openink_shape::recognize::Recognizer;
use std::sync::OnceLock;

fn recognizer() -> &'static Recognizer {
    static R: OnceLock<Recognizer> = OnceLock::new();
    R.get_or_init(Recognizer::builtin)
}

/// Split the flat samples into strokes; `None` when the lengths disagree or a value is not finite.
/// The recognisers' ink arguments (`xyt`, `lens`) as strokes; on malformed input an IllegalArgumentException
/// naming `who` is raised and `None` returned.
pub fn read_ink(env: &mut JNIEnv, xyt: &JFloatArray, lens: &JIntArray, who: &str) -> Option<Vec<Vec<Sample>>> {
    let strokes = jvm::read_floats(env, xyt).zip(jvm::read_ints(env, lens)).and_then(|(a, b)| strokes_from(&a, &b));
    if strokes.is_none() {
        jvm::throw(env, jvm::ILLEGAL_ARGUMENT, &format!("{who}: malformed ink arrays"));
    }
    strokes
}

pub fn strokes_from(xyt: &[f32], lens: &[i32]) -> Option<Vec<Vec<Sample>>> {
    if xyt.len() % 3 != 0 || !xyt.iter().all(|v| v.is_finite()) {
        return None;
    }
    let mut out = Vec::with_capacity(lens.len());
    let mut at = 0usize;
    for &n in lens {
        let n = usize::try_from(n).ok()?;
        let end = at.checked_add(n.checked_mul(3)?)?;
        let s = xyt.get(at..end)?;
        out.push(s.chunks_exact(3).map(|c| Sample { x: c[0], y: c[1], t: c[2] }).collect());
        at = end;
    }
    (at == xyt.len()).then_some(out)
}

#[no_mangle]
pub extern "system" fn Java_org_openink_shape_ShapeRecognizer_nativeRecognize(
    mut env: JNIEnv,
    _c: JClass,
    xyt: JFloatArray,
    lens: JIntArray,
) -> jfloatArray {
    let Some(strokes) = read_ink(&mut env, &xyt, &lens, "ShapeRecognizer") else { return std::ptr::null_mut() };
    jvm::write_floats(&env, &openink_shape::wire::encode(&recognizer().recognize(&strokes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split() {
        let s = strokes_from(&[0.0, 0.0, 0.0, 1.0, 1.0, 5.0, 2.0, 2.0, 9.0], &[2, 1]).unwrap();
        assert_eq!((s.len(), s[0].len(), s[1][0].t), (2, 2, 9.0));
        assert!(strokes_from(&[0.0; 9], &[2]).is_none());
        assert!(strokes_from(&[0.0; 9], &[4]).is_none());
        assert!(strokes_from(&[0.0; 9], &[-1, 4]).is_none());
        assert!(strokes_from(&[f32::NAN, 0.0, 0.0], &[1]).is_none());
    }
}

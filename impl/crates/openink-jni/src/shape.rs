//! JNI side of `org.openink.shape.ShapeRecognizer` (crates/openink-shape; replaces MyScript Shape).
//! Input: `xyt float[3 n]` (x, y, t ms) for all strokes back to back, `lens int[]` samples per stroke.
//! Output: `openink_shape::wire::encode`, or null for malformed input (with an exception thrown).

use jni::objects::{JClass, JFloatArray, JIntArray};
use jni::sys::jfloatArray;
use jni::JNIEnv;
use openink_shape::features::Sample;
use openink_shape::recognize::Recognizer;
use std::sync::OnceLock;

fn recognizer() -> &'static Recognizer {
    static R: OnceLock<Recognizer> = OnceLock::new();
    R.get_or_init(Recognizer::builtin)
}

/// Split the flat samples into strokes; `None` when the lengths disagree or a value is not finite.
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
    let read = |env: &mut JNIEnv| -> Option<(Vec<f32>, Vec<i32>)> {
        let mut a = vec![0f32; env.get_array_length(&xyt).ok()? as usize];
        env.get_float_array_region(&xyt, 0, &mut a).ok()?;
        let mut b = vec![0i32; env.get_array_length(&lens).ok()? as usize];
        env.get_int_array_region(&lens, 0, &mut b).ok()?;
        Some((a, b))
    };
    let Some(strokes) = read(&mut env).and_then(|(a, b)| strokes_from(&a, &b)) else {
        let _ = env.throw_new("java/lang/IllegalArgumentException", "ShapeRecognizer: malformed ink arrays");
        return std::ptr::null_mut();
    };
    let v = openink_shape::wire::encode(&recognizer().recognize(&strokes));
    let Ok(arr) = env.new_float_array(v.len() as i32) else { return std::ptr::null_mut() };
    if env.set_float_array_region(&arr, 0, &v).is_err() {
        return std::ptr::null_mut();
    }
    arr.into_raw()
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

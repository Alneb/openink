//! JNI side of `org.openink.stroke.StrokeEngine`: one reusable engine per pen setting.
//! Wire formats (shared with StrokeEngine.java):
//! - config `float[8]`: width, pressureSensitivity, smoothLevel, velocitySensitivity, tiltEnabled (0/1), tiltScale,
//!   dpi, minWidth (the older `float[6]` is still read: dpi 300, minWidth 0.001)
//! - samples `float[6 n]`: x, y, pressure, tiltX, tiltY, tMs (ms since pen-down)
//! - ink `float[4 + k]`: left, top, right, bottom (NaN when empty), then `Ink::data`

use jni::objects::{JByteArray, JClass, JFloatArray};
use jni::sys::{jbyteArray, jfloatArray, jint, jlong};
use jni::JNIEnv;

use crate::jvm::{read_floats, throw, write_floats, ILLEGAL_ARGUMENT, ILLEGAL_STATE};
use openink_stroke::{raster, ActiveStroke, Ink, PenConfig, PenKind, Sample, Stroke};

pub const CONFIG_LEN: usize = 8;
pub const SAMPLE_LEN: usize = 6;

pub fn config_from(v: &[f32]) -> Option<PenConfig> {
    let (dpi, min_width) = match v.len() {
        8 => (v[6], v[7]),
        6 => (300.0, 0.001),
        _ => return None,
    };
    let &[width, pressure_sensitivity, smooth_level, velocity_sensitivity, tilt, tilt_scale] = &v[..6] else { return None };
    Some(PenConfig { width, pressure_sensitivity, smooth_level, velocity_sensitivity, tilt_enabled: tilt != 0.0, tilt_scale, dpi, min_width })
}

/// `None` for a ragged array or any NaN/infinite value (one NaN would poison the whole stroke).
pub fn samples_from(v: &[f32]) -> Option<Vec<Sample>> {
    if v.len() % SAMPLE_LEN != 0 || !v.iter().all(|x| x.is_finite()) {
        return None;
    }
    Some(v.chunks_exact(SAMPLE_LEN).map(|s| Sample { x: s[0], y: s[1], pressure: s[2], tilt_x: s[3], tilt_y: s[4], t_ms: s[5] }).collect())
}

pub fn encode(ink: &Ink) -> Vec<f32> {
    let mut out = Vec::with_capacity(4 + ink.data.len());
    out.extend(ink.bounds.unwrap_or([f32::NAN; 4]));
    out.extend_from_slice(&ink.data);
    out
}

/// The engine between strokes holds only the template; during one it holds the live stroke.
struct Engine {
    template: Stroke,
    active: Option<ActiveStroke>,
}

/// The engine behind a handle. SAFETY (callers): `h` comes from nativeCreate and is freed only
/// by nativeFree; StrokeEngine.java confines each engine to one thread and zeroes it on free.
unsafe fn engine<'a>(h: jlong) -> &'a mut Engine {
    &mut *(h as *mut Engine)
}

fn samples_arg(env: &mut JNIEnv, a: &JFloatArray) -> Option<Vec<Sample>> {
    let v = read_floats(env, a);
    let s = v.as_deref().and_then(samples_from);
    if s.is_none() {
        throw(env, ILLEGAL_ARGUMENT, "samples must be float[6 n], all finite");
    }
    s
}

#[no_mangle]
pub extern "system" fn Java_org_openink_stroke_StrokeEngine_nativeCreate(mut env: JNIEnv, _c: JClass, kind: jint, cfg: JFloatArray) -> jlong {
    let Some(kind) = PenKind::from_code(kind) else {
        throw(&mut env, ILLEGAL_ARGUMENT, &format!("unknown pen kind {kind}"));
        return 0;
    };
    let Some(cfg) = read_floats(&env, &cfg).as_deref().and_then(config_from) else {
        throw(&mut env, ILLEGAL_ARGUMENT, "config must be float[6] or float[8]");
        return 0;
    };
    Box::into_raw(Box::new(Engine { template: Stroke::new(kind, cfg), active: None })) as jlong
}

#[no_mangle]
pub extern "system" fn Java_org_openink_stroke_StrokeEngine_nativeDown(mut env: JNIEnv, _c: JClass, h: jlong, sample: JFloatArray) -> jfloatArray {
    let e = unsafe { engine(h) };
    let Some(s) = samples_arg(&mut env, &sample) else { return std::ptr::null_mut() };
    let [s] = s[..] else {
        throw(&mut env, ILLEGAL_ARGUMENT, "down takes one sample");
        return std::ptr::null_mut();
    };
    if e.active.is_some() {
        throw(&mut env, ILLEGAL_STATE, "down() during a stroke");
        return std::ptr::null_mut();
    }
    let (a, ink) = e.template.down(s);
    e.active = Some(a);
    write_floats(&env, &encode(&ink))
}

#[no_mangle]
pub extern "system" fn Java_org_openink_stroke_StrokeEngine_nativeMove(mut env: JNIEnv, _c: JClass, h: jlong, samples: JFloatArray) -> jfloatArray {
    let e = unsafe { engine(h) };
    let Some(s) = samples_arg(&mut env, &samples) else { return std::ptr::null_mut() };
    let Some(a) = e.active.as_mut() else {
        throw(&mut env, ILLEGAL_STATE, "move() without down()");
        return std::ptr::null_mut();
    };
    let ink = a.moved(&s);
    write_floats(&env, &encode(&ink))
}

#[no_mangle]
pub extern "system" fn Java_org_openink_stroke_StrokeEngine_nativeUp(mut env: JNIEnv, _c: JClass, h: jlong, sample: JFloatArray) -> jfloatArray {
    let e = unsafe { engine(h) };
    let Some(s) = samples_arg(&mut env, &sample) else { return std::ptr::null_mut() };
    // Check the argument before taking the stroke, so a bad call leaves it live.
    let [s] = s[..] else {
        throw(&mut env, ILLEGAL_ARGUMENT, "up takes one sample");
        return std::ptr::null_mut();
    };
    let Some(a) = e.active.take() else {
        throw(&mut env, ILLEGAL_STATE, "up() without down()");
        return std::ptr::null_mut();
    };
    let ink = a.up(s);
    write_floats(&env, &encode(&ink))
}

/// Whole recorded stroke at once (redraw); does not touch a live stroke.
#[no_mangle]
pub extern "system" fn Java_org_openink_stroke_StrokeEngine_nativeRender(mut env: JNIEnv, _c: JClass, h: jlong, samples: JFloatArray) -> jfloatArray {
    let e = unsafe { engine(h) };
    let Some(s) = samples_arg(&mut env, &samples) else { return std::ptr::null_mut() };
    let ink = e.template.render(&s);
    write_floats(&env, &encode(&ink))
}

#[no_mangle]
pub extern "system" fn Java_org_openink_stroke_StrokeEngine_nativeFree(_env: JNIEnv, _c: JClass, h: jlong) {
    if h != 0 {
        drop(unsafe { Box::from_raw(h as *mut Engine) });
    }
}

#[no_mangle]
pub extern "system" fn Java_org_openink_stroke_StrokeEngine_nativeGrainMask(env: JNIEnv, _c: JClass, size: jint) -> jbyteArray {
    let m = raster::grain_mask(size.clamp(2, 256) as usize);
    env.byte_array_from_slice(&m).map(JByteArray::into_raw).unwrap_or(std::ptr::null_mut())
}

pub fn config_to(c: &PenConfig) -> [f32; CONFIG_LEN] {
    [c.width, c.pressure_sensitivity, c.smooth_level, c.velocity_sensitivity, f32::from(u8::from(c.tilt_enabled)), c.tilt_scale, c.dpi, c.min_width]
}

/// `LivePen` wire: style, sfWidth, exact (0/1), n, params[n], settled config `float[8]`.
pub fn encode_live_pen(p: &openink_stroke::sf::LivePen) -> Vec<f32> {
    let mut v = vec![p.style.code() as f32, p.sf_width, f32::from(u8::from(p.exact)), p.sf_params.len() as f32];
    v.extend_from_slice(&p.sf_params);
    v.extend_from_slice(&config_to(&p.settled));
    v
}

#[no_mangle]
pub extern "system" fn Java_org_openink_stroke_LivePen_nativeLivePen(mut env: JNIEnv, _c: JClass, kind: jint, cfg: JFloatArray) -> jfloatArray {
    let (Some(kind), Some(cfg)) = (PenKind::from_code(kind), read_floats(&env, &cfg).as_deref().and_then(config_from)) else {
        throw(&mut env, ILLEGAL_ARGUMENT, "LivePen: unknown pen kind, or config not float[6]/float[8]");
        return std::ptr::null_mut();
    };
    write_floats(&env, &encode_live_pen(&openink_stroke::sf::live_pen(kind, &cfg)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_pen_wire() {
        let want = config_from(&[10.0, 0.3, 0.6, 0.5, 0.0, 3.0]).unwrap();
        let v = encode_live_pen(&openink_stroke::sf::live_pen(PenKind::Fountain, &want));
        assert_eq!(&v[..4], &[1.0, 7.0, 1.0, 2.0]);
        assert_eq!(v.len(), 4 + 2 + CONFIG_LEN);
        let settled = config_from(&v[6..]).unwrap();
        assert_eq!((settled.width, settled.dpi, settled.min_width), (10.0, 320.0, 1.0));
    }

    #[test]
    fn wire_formats_round_trip() {
        let c = config_from(&[7.0, 0.3, 0.6, 0.5, 1.0, 3.0]).unwrap();
        assert!(c.tilt_enabled && c.width == 7.0);
        assert!(config_from(&[1.0; 5]).is_none());
        assert!(config_from(&[1.0; 7]).is_none());
        assert_eq!(c.dpi, 300.0, "a 6-float config keeps the 300 dpi default");
        let sf = config_from(&[10.0, 0.3, 0.2, 0.5, 0.0, 3.0, 320.0, 1.0]).unwrap();
        assert!(sf.dpi == 320.0 && sf.min_width == 1.0);
        assert!(samples_from(&[0.0; 7]).is_none());
        assert!(samples_from(&[f32::NAN, 2.0, 0.5, 3.0, 4.0, 9.0]).is_none());
        let s = samples_from(&[1.0, 2.0, 0.5, 3.0, 4.0, 9.0]).unwrap();
        assert_eq!((s[0].y, s[0].tilt_y, s[0].t_ms), (2.0, 4.0, 9.0));
        let ink = Stroke::new(PenKind::Fountain, c).render(&s);
        let enc = encode(&ink);
        assert_eq!(enc.len(), 4 + ink.data.len());
        assert!(enc[0] < 1.0 && enc[2] > 1.0);
    }

    /// Hostile configs and empty sample runs reach this code from Java: a panic here would abort
    /// the app (no unwinding across extern "system"), and NaN ink would poison the canvas.
    #[test]
    fn degenerate_input_never_panics_or_yields_nan() {
        let good = config_from(&[7.0, 0.3, 0.6, 0.5, 1.0, 3.0]).unwrap();
        let two = samples_from(&[1.0, 2.0, 0.5, 3.0, 4.0, 9.0, 50.0, 60.0, 0.5, 3.0, 4.0, 19.0]).unwrap();
        for code in 0..7 {
            let kind = PenKind::from_code(code).unwrap();
            let st = Stroke::new(kind, good);
            assert!(st.render(&[]).bounds.is_none(), "{kind:?}: empty render has no ink");
            let (mut a, _) = st.down(two[0]);
            let _ = a.moved(&[]);
            let _ = a.up(two[1]);
            for bad in [f32::NAN, -5.0, 0.0, f32::INFINITY, 1e30] {
                for field in [0, 1, 2, 3, 6, 7] {
                    let mut v = [7.0, 0.3, 0.6, 0.5, 1.0, 3.0, 300.0, 0.001];
                    v[field] = bad;
                    let ink = Stroke::new(kind, config_from(&v).unwrap()).render(&two);
                    assert!(encode(&ink).iter().all(|x| x.is_finite()), "{kind:?}: config[{field}] = {bad} gave non-finite ink");
                }
            }
        }
    }
}

//! JNI side of `org.openink.epd.EpdEngine` (crates/openink-epdc). Wire formats, shared with
//! EpdEngine.java:
//! - request intent: `Intent::code()` 0..4, or `RAW` (-1) with the Onyx mode int in `raw_mode`
//! - poll result `int[6 n]`: left, top, width, height, Onyx mode, reason code

use jni::objects::JClass;
use jni::sys::{jint, jintArray, jlong};
use jni::JNIEnv;
use openink_core::UpdateMode;
use openink_epdc::{Engine, Intent, Millis, Policy, Reason, Rect, Update};
use std::num::NonZeroU32;

pub const RAW: i32 = -1;
pub const UPDATE_LEN: usize = 6;

pub const fn reason_code(r: Reason) -> i32 {
    match r {
        Reason::Request => 0,
        Reason::GcInterval => 1,
        Reason::Cleanup => 2,
    }
}

/// `Err` names what is wrong, for the IllegalArgumentException.
pub fn intent_from(code: i32, raw_mode: i32) -> Result<Intent, String> {
    if code == RAW {
        return UpdateMode::from_onyx(raw_mode).map(Intent::Raw).map_err(|e| e.to_string());
    }
    Intent::from_code(code).ok_or_else(|| format!("unknown intent {code}"))
}

pub fn encode(updates: &[Update]) -> Vec<i32> {
    let mut out = Vec::with_capacity(updates.len() * UPDATE_LEN);
    for u in updates {
        out.extend([u.rect.left(), u.rect.top(), u.rect.width(), u.rect.height(), u.mode.to_onyx(), reason_code(u.reason)]);
    }
    out
}

fn throw(env: &mut JNIEnv, msg: &str) {
    let _ = env.throw_new("java/lang/IllegalArgumentException", msg);
}

/// SAFETY (callers): `h` comes from nativeCreate and is freed only by nativeFree; EpdEngine.java
/// confines an engine to one thread (its Handler's) and zeroes the handle on close.
unsafe fn engine<'a>(h: jlong) -> &'a mut Engine {
    &mut *(h as *mut Engine)
}

fn now(ms: jlong) -> Millis {
    Millis(ms.max(0) as u64)
}

#[no_mangle]
pub extern "system" fn Java_org_openink_epd_EpdEngine_nativeCreate(mut env: JNIEnv, _c: JClass, width: jint, height: jint) -> jlong {
    let Some(screen) = Rect::new(0, 0, width, height) else {
        throw(&mut env, &format!("empty screen {width}x{height}"));
        return 0;
    };
    Box::into_raw(Box::new(Engine::new(screen, Policy::default()))) as jlong
}

#[no_mangle]
#[allow(clippy::too_many_arguments)] // flat JNI signature: no per-call object allocation
pub extern "system" fn Java_org_openink_epd_EpdEngine_nativeRequest(
    mut env: JNIEnv, _c: JClass, h: jlong, l: jint, t: jint, r: jint, b: jint, intent: jint, raw_mode: jint, ms: jlong,
) {
    let e = unsafe { engine(h) };
    let Some(rect) = Rect::new(l, t, r, b) else {
        throw(&mut env, &format!("empty rect ({l},{t})-({r},{b})"));
        return;
    };
    match intent_from(intent, raw_mode) {
        Ok(i) => e.request(rect, i, now(ms)),
        Err(m) => throw(&mut env, &m),
    }
}

#[no_mangle]
pub extern "system" fn Java_org_openink_epd_EpdEngine_nativePenDown(_env: JNIEnv, _c: JClass, h: jlong, ms: jlong) {
    unsafe { engine(h) }.pen_down(now(ms));
}

/// An empty stroke box (r <= l) means "no ink to repaint".
#[no_mangle]
pub extern "system" fn Java_org_openink_epd_EpdEngine_nativePenUp(_env: JNIEnv, _c: JClass, h: jlong, l: jint, t: jint, r: jint, b: jint, ms: jlong) {
    unsafe { engine(h) }.pen_up(Rect::new(l, t, r, b), now(ms));
}

#[no_mangle]
pub extern "system" fn Java_org_openink_epd_EpdEngine_nativePoll(env: JNIEnv, _c: JClass, h: jlong, ms: jlong) -> jintArray {
    to_java(env, &unsafe { engine(h) }.poll(now(ms)))
}

fn to_java(env: JNIEnv, updates: &[Update]) -> jintArray {
    let v = encode(updates);
    let Ok(a) = env.new_int_array(v.len() as i32) else { return std::ptr::null_mut() };
    if env.set_int_array_region(&a, 0, &v).is_err() {
        return std::ptr::null_mut();
    }
    a.into_raw()
}

/// Like nativePoll, but sends everything queued at once (EpdController.waitForUpdateFinished).
#[no_mangle]
pub extern "system" fn Java_org_openink_epd_EpdEngine_nativeFlush(env: JNIEnv, _c: JClass, h: jlong, ms: jlong) -> jintArray {
    to_java(env, &unsafe { engine(h) }.flush(now(ms)))
}

/// -1 when nothing is scheduled.
#[no_mangle]
pub extern "system" fn Java_org_openink_epd_EpdEngine_nativeNextDeadline(_env: JNIEnv, _c: JClass, h: jlong) -> jlong {
    unsafe { engine(h) }.next_deadline().map_or(-1, |m| m.0 as jlong)
}

/// 0 turns the GC interval off.
#[no_mangle]
pub extern "system" fn Java_org_openink_epd_EpdEngine_nativeSetGcEvery(_env: JNIEnv, _c: JClass, h: jlong, n: jint) {
    unsafe { engine(h) }.set_gc_every(NonZeroU32::new(n.max(0) as u32));
}

/// `RAW` (-1) clears the app-scope mode.
#[no_mangle]
pub extern "system" fn Java_org_openink_epd_EpdEngine_nativeSetScopeMode(mut env: JNIEnv, _c: JClass, h: jlong, mode: jint) {
    let e = unsafe { engine(h) };
    if mode == RAW {
        return e.set_scope_mode(None);
    }
    match UpdateMode::from_onyx(mode) {
        Ok(m) => e.set_scope_mode(Some(m)),
        Err(err) => throw(&mut env, &err.to_string()),
    }
}

#[no_mangle]
pub extern "system" fn Java_org_openink_epd_EpdEngine_nativeFree(_env: JNIEnv, _c: JClass, h: jlong) {
    if h != 0 {
        drop(unsafe { Box::from_raw(h as *mut Engine) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_round_trip() {
        assert_eq!(intent_from(0, 0), Ok(Intent::Ui));
        assert_eq!(intent_from(RAW, UpdateMode::GC.to_onyx()), Ok(Intent::Raw(UpdateMode::GC)));
        assert!(intent_from(RAW, 7).is_err());
        assert!(intent_from(9, 0).is_err());
        let u = Update { rect: Rect::new(1, 2, 11, 22).unwrap(), mode: UpdateMode::DU, reason: Reason::Cleanup };
        assert_eq!(encode(&[u]), [1, 2, 10, 20, 1, 2]);
    }
}

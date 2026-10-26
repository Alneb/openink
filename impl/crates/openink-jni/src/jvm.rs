//! Marshalling shared by the JNI entry points: Java arrays in and out, and exceptions. A failed JNI call returns
//! `None` / null, which Java sees as the pending exception (or a null result).

use jni::objects::{JFloatArray, JIntArray};
use jni::sys::{jfloatArray, jintArray};
use jni::JNIEnv;

pub const ILLEGAL_ARGUMENT: &str = "java/lang/IllegalArgumentException";
pub const ILLEGAL_STATE: &str = "java/lang/IllegalStateException";
pub const IO: &str = "java/io/IOException";

/// Raise `class` in Java; the native function then returns and Java throws it.
pub fn throw(env: &mut JNIEnv, class: &str, msg: &str) {
    let _ = env.throw_new(class, msg);
}

pub fn read_floats(env: &JNIEnv, a: &JFloatArray) -> Option<Vec<f32>> {
    let mut v = vec![0f32; env.get_array_length(a).ok()? as usize];
    env.get_float_array_region(a, 0, &mut v).ok()?;
    Some(v)
}

pub fn read_ints(env: &JNIEnv, a: &JIntArray) -> Option<Vec<i32>> {
    let mut v = vec![0i32; env.get_array_length(a).ok()? as usize];
    env.get_int_array_region(a, 0, &mut v).ok()?;
    Some(v)
}

pub fn write_floats(env: &JNIEnv, v: &[f32]) -> jfloatArray {
    let Ok(a) = env.new_float_array(v.len() as i32) else { return std::ptr::null_mut() };
    if env.set_float_array_region(&a, 0, v).is_err() {
        return std::ptr::null_mut();
    }
    a.into_raw()
}

pub fn write_ints(env: &JNIEnv, v: &[i32]) -> jintArray {
    let Ok(a) = env.new_int_array(v.len() as i32) else { return std::ptr::null_mut() };
    if env.set_int_array_region(&a, 0, v).is_err() {
        return std::ptr::null_mut();
    }
    a.into_raw()
}

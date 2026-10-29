//! JNI bindings for `io.github.rayhungkao.pqtunnel.Native`. A session lives on the Rust heap and
//! Kotlin holds it as an opaque `Long` handle until it calls `free`.

use std::sync::Mutex;

use jni::objects::{JByteArray, JClass};
use jni::sys::{jbyteArray, jlong};
use jni::JNIEnv;

use crate::Session;

struct Handle {
    // Kotlin seals on the TUN-reader thread and opens on the socket-reader thread.
    session: Mutex<Session>,
    init: Vec<u8>,
}

fn handle<'a>(h: jlong) -> &'a Handle {
    // Safety: `h` came from `connect` and Kotlin stops using it after `free`.
    unsafe { &*(h as *const Handle) }
}

fn to_java(env: &mut JNIEnv, bytes: &[u8]) -> jbyteArray {
    env.byte_array_from_slice(bytes).map(JByteArray::into_raw).unwrap_or(std::ptr::null_mut())
}

/// Returns 0 when the server key is malformed.
#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_connect(
    env: JNIEnv,
    _: JClass,
    server_public: JByteArray,
) -> jlong {
    let Ok(bytes) = env.convert_byte_array(&server_public) else { return 0 };
    match crate::connect(&bytes) {
        Ok((session, init)) => Box::into_raw(Box::new(Handle { session: Mutex::new(session), init })) as jlong,
        Err(_) => 0,
    }
}

#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_initPacket(
    mut env: JNIEnv,
    _: JClass,
    h: jlong,
) -> jbyteArray {
    to_java(&mut env, &handle(h).init)
}

#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_seal(
    mut env: JNIEnv,
    _: JClass,
    h: jlong,
    plain: JByteArray,
) -> jbyteArray {
    let Ok(bytes) = env.convert_byte_array(&plain) else { return std::ptr::null_mut() };
    let packet = handle(h).session.lock().unwrap().seal(&bytes);
    to_java(&mut env, &packet)
}

/// Returns null for packets that fail authentication or replay checks; the caller drops them.
#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_open(
    mut env: JNIEnv,
    _: JClass,
    h: jlong,
    packet: JByteArray,
) -> jbyteArray {
    let Ok(bytes) = env.convert_byte_array(&packet) else { return std::ptr::null_mut() };
    match handle(h).session.lock().unwrap().open(&bytes) {
        Ok(plain) => to_java(&mut env, &plain),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_free(_: JNIEnv, _: JClass, h: jlong) {
    if h != 0 {
        drop(unsafe { Box::from_raw(h as *mut Handle) });
    }
}

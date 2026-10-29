//! JNI bindings for `io.github.rayhungkao.pqtunnel.Native`. A session lives on the Rust heap and
//! Kotlin holds it as an opaque `Long` handle until it calls `free`.

use std::sync::Mutex;

use jni::objects::{JByteArray, JClass};
use jni::sys::{jboolean, jbyteArray, jlong};
use jni::JNIEnv;

use crate::Client;

struct Handle {
    // Kotlin seals on the TUN-reader thread and receives on the socket-reader thread.
    client: Mutex<Client>,
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
        Ok((client, init)) => Box::into_raw(Box::new(Handle { client: Mutex::new(client), init })) as jlong,
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

/// Returns null until the server's ACK has arrived.
#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_seal(
    mut env: JNIEnv,
    _: JClass,
    h: jlong,
    plain: JByteArray,
) -> jbyteArray {
    let Ok(bytes) = env.convert_byte_array(&plain) else { return std::ptr::null_mut() };
    match handle(h).client.lock().unwrap().seal(&bytes) {
        Some(packet) => to_java(&mut env, &packet),
        None => std::ptr::null_mut(),
    }
}

/// Returns the IP packet inside DATA, or null for an ACK (handled here) and for packets that fail
/// authentication or replay checks; the caller drops those.
#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_open(
    mut env: JNIEnv,
    _: JClass,
    h: jlong,
    packet: JByteArray,
) -> jbyteArray {
    let Ok(bytes) = env.convert_byte_array(&packet) else { return std::ptr::null_mut() };
    match handle(h).client.lock().unwrap().receive(&bytes) {
        Ok(Some(plain)) => to_java(&mut env, &plain),
        _ => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_ready(_: JNIEnv, _: JClass, h: jlong) -> jboolean {
    jboolean::from(handle(h).client.lock().unwrap().is_ready())
}

#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_free(_: JNIEnv, _: JClass, h: jlong) {
    if h != 0 {
        drop(unsafe { Box::from_raw(h as *mut Handle) });
    }
}

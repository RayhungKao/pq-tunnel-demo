//! JNI bindings for `io.github.rayhungkao.pqtunnel.Native`. A session lives in a handle table and
//! Kotlin holds its number as a `Long` until it calls `free`. Every call looks the number up, so a
//! call with a freed or unknown handle returns null or false instead of touching freed memory.

use std::sync::{Arc, LazyLock, Mutex};

use jni::objects::{JByteArray, JClass};
use jni::sys::{jboolean, jbyteArray, jlong, JNI_FALSE};
use jni::JNIEnv;

use crate::handles::Handles;
use crate::Client;

struct Session {
    // Kotlin seals on the TUN-reader thread and receives on the socket-reader thread.
    client: Mutex<Client>,
    init: Vec<u8>,
}

static SESSIONS: LazyLock<Handles<Session>> = LazyLock::new(Handles::new);

fn session(h: jlong) -> Option<Arc<Session>> {
    SESSIONS.get(h)
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
        Ok((client, init)) => SESSIONS.insert(Session { client: Mutex::new(client), init }),
        Err(_) => 0,
    }
}

/// Returns null for a freed or unknown handle.
#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_initPacket(
    mut env: JNIEnv,
    _: JClass,
    h: jlong,
) -> jbyteArray {
    match session(h) {
        Some(s) => to_java(&mut env, &s.init),
        None => std::ptr::null_mut(),
    }
}

/// Returns null until the server's ACK has arrived, and for a freed or unknown handle.
#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_seal(
    mut env: JNIEnv,
    _: JClass,
    h: jlong,
    plain: JByteArray,
) -> jbyteArray {
    let Some(s) = session(h) else { return std::ptr::null_mut() };
    let Ok(bytes) = env.convert_byte_array(&plain) else { return std::ptr::null_mut() };
    let sealed = s.client.lock().unwrap().seal(&bytes);
    match sealed {
        Some(packet) => to_java(&mut env, &packet),
        None => std::ptr::null_mut(),
    }
}

/// Returns the IP packet inside DATA, or null for an ACK (handled here), for packets that fail
/// authentication or replay checks, and for a freed or unknown handle; the caller drops those.
#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_open(
    mut env: JNIEnv,
    _: JClass,
    h: jlong,
    packet: JByteArray,
) -> jbyteArray {
    let Some(s) = session(h) else { return std::ptr::null_mut() };
    let Ok(bytes) = env.convert_byte_array(&packet) else { return std::ptr::null_mut() };
    let opened = s.client.lock().unwrap().receive(&bytes);
    match opened {
        Ok(Some(plain)) => to_java(&mut env, &plain),
        _ => std::ptr::null_mut(),
    }
}

/// False until the ACK has arrived, and for a freed or unknown handle.
#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_ready(_: JNIEnv, _: JClass, h: jlong) -> jboolean {
    session(h).map_or(JNI_FALSE, |s| jboolean::from(s.client.lock().unwrap().is_ready()))
}

/// Freeing twice, or freeing 0, does nothing.
#[no_mangle]
pub extern "system" fn Java_io_github_rayhungkao_pqtunnel_Native_free(_: JNIEnv, _: JClass, h: jlong) {
    SESSIONS.remove(h);
}

//! Android JNI boundary.
//!
//! This is the only crate that produces a `cdylib`, and it exists so that
//! `madari-tv` can stay a plain library. Cargo builds *every* declared crate type
//! of an in-workspace path dependency, so a `cdylib` on the shared crate made
//! `cargo build --target aarch64-apple-ios` try to link an iOS dylib, which does
//! not exist and which the Swift toolchain's `ld64.lld` refuses to produce.
//!
//! The library name stays `madari_tv` so the produced file is still
//! `libmadari_tv.so` and `System.loadLibrary("madari_tv")` keeps working.
//!
//! Policies and persistence stay in the shared Rust libraries. The whole crate
//! is gated to Android, because `jni` and the platform verifier's Android module
//! only exist for that target.
#![cfg(target_os = "android")]

use jni::{
    JNIEnv,
    objects::{JByteArray, JClass, JString},
    sys::{jint, jlong, jstring},
};
use madari_model::*;
use madari_tv::{Bridge, ReaderRead};
use std::sync::OnceLock;

/// The core reports invalid input as a typed error; the JNI layer only needs the
/// constructor, which the shared crate keeps private.
fn invalid(message: &str) -> Error {
    Error::new(ErrorCode::InvalidInput, message)
}

/// The process-wide bridge. JNI has no context object to hang it off, so it lives
/// here for the lifetime of the process.
static BRIDGE: OnceLock<Bridge> = OnceLock::new();
fn bridge() -> Result<&'static Bridge> {
    BRIDGE
        .get()
        .ok_or_else(|| invalid("Native library is not initialized"))
}
fn exception(env: &mut JNIEnv, error: impl std::fmt::Display) {
    let _ = env.throw_new("java/io/IOException", error.to_string());
}
fn guarded<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err(invalid("Native operation failed")))
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_madari_tv_core_NativeCore_initialize(
    mut env: JNIEnv,
    _: JClass,
    path: JString,
) {
    let result = guarded(|| {
        if BRIDGE.get().is_some() {
            return Ok(());
        }
        let path: String = env
            .get_string(&path)
            .map_err(|_| invalid("Invalid storage path"))?
            .into();
        let instance = Bridge::open(path.into())?;
        BRIDGE
            .set(instance)
            .map_err(|_| invalid("Already initialized"))
    });
    if let Err(error) = result {
        exception(&mut env, error);
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_madari_tv_core_NativeCore_dispatch(
    mut env: JNIEnv,
    _: JClass,
    operation: JString,
    args: JString,
) -> jstring {
    let result = guarded(|| {
        let operation: String = env
            .get_string(&operation)
            .map_err(|_| invalid("Invalid operation"))?
            .into();
        let args: String = env
            .get_string(&args)
            .map_err(|_| invalid("Invalid arguments"))?
            .into();
        let value = bridge()?.call(
            &operation,
            serde_json::from_str(&args).map_err(|_| invalid("Invalid JSON"))?,
        )?;
        Ok(value.to_string())
    });
    match result {
        Ok(value) => match env.new_string(value) {
            Ok(value) => value.into_raw(),
            Err(error) => {
                exception(&mut env, error);
                std::ptr::null_mut()
            }
        },
        Err(error) => {
            exception(&mut env, error);
            std::ptr::null_mut()
        }
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_madari_tv_core_NativeCore_openMedia(
    mut env: JNIEnv,
    _: JClass,
    uri: JString,
    position: jlong,
) -> jlong {
    let result = guarded(|| {
        if position < 0 {
            return Err(invalid("Negative stream position"));
        }
        let uri: String = env
            .get_string(&uri)
            .map_err(|_| invalid("Invalid media URI"))?
            .into();
        bridge()?.open_reader(&uri, position as u64)
    });
    match result {
        Ok(id) => id,
        Err(error) => {
            exception(&mut env, error);
            0
        }
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_madari_tv_core_NativeCore_mediaLength(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
) -> jlong {
    let result = guarded(|| Ok(bridge()?.reader_length(id)?.min(i64::MAX as u64) as i64));
    match result {
        Ok(n) => n,
        Err(e) => {
            exception(&mut env, e);
            -1
        }
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_madari_tv_core_NativeCore_readMedia(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
    output: JByteArray,
    offset: jint,
    length: jint,
) -> jint {
    let result = guarded(|| {
        let size = env
            .get_array_length(&output)
            .map_err(|_| invalid("Invalid output buffer"))?;
        if offset < 0 || length < 0 || offset.checked_add(length).is_none_or(|end| end > size) {
            return Err(invalid("Invalid read bounds"));
        }
        // Yields periodically to let the Media3 loading thread observe cancellation.
        // -2 is internal "piece not available yet", never end-of-stream.
        let data = match bridge()?.read_reader(id, length as usize)? {
            ReaderRead::Pending => return Ok(-2),
            ReaderRead::Eof => return Ok(-1),
            ReaderRead::Data(data) => data,
        };
        let bytes: Vec<i8> = data.iter().map(|&v| v as i8).collect();
        env.set_byte_array_region(&output, offset, &bytes)
            .map_err(|_| invalid("Could not copy media bytes"))?;
        Ok(bytes.len() as i32)
    });
    match result {
        Ok(n) => n,
        Err(e) => {
            exception(&mut env, e);
            -1
        }
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_madari_tv_core_NativeCore_closeMedia(
    mut env: JNIEnv,
    _: JClass,
    id: jlong,
) {
    if let Err(e) = guarded(|| {
        bridge()?.close_reader(id);
        Ok(())
    }) {
        exception(&mut env, e);
    }
}

// reqwest 0.13 uses Android's trust store through rustls-platform-verifier.
// Its Java classes and application context must be installed before the first HTTPS call.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_madari_tv_core_NativeCore_initializeTls<'local>(
    mut env: jni_platform::EnvUnowned<'local>,
    _: jni_platform::objects::JClass<'local>,
    context: jni_platform::objects::JObject<'local>,
) {
    env.with_env(|env| rustls_platform_verifier::android::init_with_env(env, context))
        .resolve::<jni_platform::errors::ThrowRuntimeExAndDefault>();
}

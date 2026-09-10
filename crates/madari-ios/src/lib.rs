//! UniFFI boundary for the iOS client.
//!
//! The shared [`madari_tv::Bridge`] owns the runtime, profile session, SQLite
//! store and torrent engine. This crate is deliberately thin: it marshals
//! arguments, maps errors into a Swift-friendly type, and exposes the internal
//! media reader that `AVAssetResourceLoader` streams torrent bytes through.
//!
//! Every operation the app can perform goes through [`dispatch`], which takes and
//! returns JSON. That keeps the contract identical to the Android JNI boundary
//! (`NativeCore.dispatch`), so policies and persistence stay in the shared core.

use madari_model::Error as CoreError;
use madari_tv::{Bridge, ReaderRead as CoreReaderRead};
use std::sync::OnceLock;

uniffi::setup_scaffolding!("madari_ios");

/// Every failure crossing into Swift, carrying the core's own message.
///
/// The core already writes user-presentable text ("Select a profile first",
/// "Position exceeds file length"), so it is passed through rather than
/// rewritten here. `flat_error` sends the message as a plain string instead of a
/// serialised variant, which keeps the Swift case a simple `Failed(message:)`.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MadariError {
    #[error("{message}")]
    Failed { message: String },
}

impl From<CoreError> for MadariError {
    fn from(error: CoreError) -> Self {
        Self::Failed {
            message: error.message,
        }
    }
}

/// Outcome of one read from an open internal media stream.
#[derive(Debug, uniffi::Enum)]
pub enum MediaRead {
    /// The torrent piece is not ready yet. Call again; this is not end of stream.
    Pending,
    /// The stream ended before `length` bytes were available.
    Eof,
    /// The bytes starting at the requested position.
    Data { bytes: Vec<u8> },
}

static BRIDGE: OnceLock<Bridge> = OnceLock::new();

fn bridge() -> Result<&'static Bridge, MadariError> {
    BRIDGE.get().ok_or(MadariError::Failed {
        message: "Native library is not initialized".into(),
    })
}

/// Opens the profile database and torrent engine under `path`.
///
/// Safe to call more than once: the first call wins, matching Android. The app
/// passes its Application Support directory.
#[uniffi::export]
pub fn initialize(path: String) -> Result<(), MadariError> {
    if BRIDGE.get().is_some() {
        return Ok(());
    }
    let instance = Bridge::open(path.into())?;
    // Losing the race is not an error: another caller opened the same store.
    let _ = BRIDGE.set(instance);
    Ok(())
}

/// Runs one core operation and returns its JSON result.
///
/// `arguments` must be a JSON object. Failures carry the core's message, which
/// the app shows to the user unchanged.
#[uniffi::export]
pub fn dispatch(operation: String, arguments: String) -> Result<String, MadariError> {
    let arguments = serde_json::from_str(&arguments).map_err(|_| MadariError::Failed {
        message: "Invalid JSON".into(),
    })?;
    let value = bridge()?.call(&operation, arguments)?;
    Ok(value.to_string())
}

/// Opens an internal media URI (`madari-internal://<token>`) as a seekable
/// reader and returns its handle.
#[uniffi::export]
pub fn open_media(uri: String, position: i64) -> Result<i64, MadariError> {
    if position < 0 {
        return Err(MadariError::Failed {
            message: "Negative stream position".into(),
        });
    }
    Ok(bridge()?.open_reader(&uri, position as u64)?)
}

/// Total length in bytes of an open internal media stream.
#[uniffi::export]
pub fn media_length(handle: i64) -> Result<i64, MadariError> {
    Ok(bridge()?.reader_length(handle)?.min(i64::MAX as u64) as i64)
}

/// Reads up to `length` bytes starting at `position`.
///
/// `AVAssetResourceLoader` asks for arbitrary ranges, so this seeks before every
/// read instead of relying on a stream position. A `Pending` result means the
/// torrent has not produced that piece yet and the request should be retried.
#[uniffi::export]
pub fn read_media(handle: i64, position: i64, length: i64) -> Result<MediaRead, MadariError> {
    if position < 0 || length < 0 {
        return Err(MadariError::Failed {
            message: "Invalid read bounds".into(),
        });
    }
    if length == 0 {
        return Ok(MediaRead::Eof);
    }
    let bridge = bridge()?;
    bridge.seek_reader(handle, position as u64)?;
    Ok(match bridge.read_reader(handle, length as usize)? {
        CoreReaderRead::Pending => MediaRead::Pending,
        CoreReaderRead::Eof => MediaRead::Eof,
        CoreReaderRead::Data(bytes) => MediaRead::Data { bytes },
    })
}

/// Releases an internal media reader. Closing an unknown handle is not an error.
#[uniffi::export]
pub fn close_media(handle: i64) {
    if let Some(bridge) = BRIDGE.get() {
        bridge.close_reader(handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the same surface the Swift client drives, without a device.
    #[test]
    fn dispatch_and_profiles_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        initialize(dir.path().to_string_lossy().into_owned()).unwrap();
        let listing: serde_json::Value =
            serde_json::from_str(&dispatch("profiles".into(), "{}".into()).unwrap()).unwrap();
        assert!(
            listing["avatars"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == "Fox.webp")
        );
        let created: serde_json::Value = serde_json::from_str(
            &dispatch(
                "create_profile".into(),
                r#"{"name":"Phone","avatar":"Fox.webp"}"#.into(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(created["name"], "Phone");
        let id = created["id"].as_str().unwrap().to_owned();
        dispatch("unlock".into(), format!(r#"{{"id":"{id}"}}"#)).unwrap();
        assert!(
            dispatch("authorize".into(), "{}".into())
                .unwrap()
                .contains("null")
        );
    }

    #[test]
    fn dispatch_rejects_invalid_input() {
        // Malformed JSON and unknown operations fail without panicking.
        assert!(dispatch("profiles".into(), "not json".into()).is_err());
        assert!(dispatch("nonsense".into(), "{}".into()).is_err());
    }

    #[test]
    fn media_reads_report_unavailable_instead_of_failing() {
        // An unknown handle is a closed reader, not a panic.
        assert!(media_length(i64::MAX).is_err());
        assert!(open_media("https://example.com/movie.mp4".into(), 0).is_err());
    }
}

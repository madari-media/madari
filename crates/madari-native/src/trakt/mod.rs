//! Trakt import and playback integration. OAuth credentials never enter core snapshots.
mod client;
mod data;
pub use client::Tokens;
pub use client::{Client, Credentials, DeviceCode, Poll};
pub use data::{Data, List, Title};

use madari_model::{Error, ErrorCode};
fn invalid(message: &str) -> Error {
    Error::new(ErrorCode::InvalidResponse, message)
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

mod scrobble;
pub use scrobble::{Action, Media, Outcome, Scrobble};

#[cfg(test)]
mod resolve_tests;

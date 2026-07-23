//! The backend error type (`SafaiError`).
//!
//! Follows `tauri-v2-guide.md` §2: a `thiserror` enum plus a manual
//! `serde::Serialize` that emits the human-readable string form so the error
//! reaches the frontend as a plain message (the `Err` branch of an `invoke`
//! promise). Keeping the serialized form a string keeps the TS side simple.

use thiserror::Error;

/// All errors the Safai commands can surface across the IPC boundary.
#[derive(Debug, Error)]
pub enum SafaiError {
    /// Filesystem / IO failure (auto-converted from `std::io::Error` via `?`).
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Any other failure, carrying a message. Command handlers stream
    /// per-item failure reasons as strings (see `DeleteEvent::Skipped`), so a
    /// single catch-all alongside `Io` covers the command-return surface.
    #[error("{0}")]
    Other(String),
}

impl serde::Serialize for SafaiError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        // The frontend receives the error as its display string.
        serializer.serialize_str(&self.to_string())
    }
}

/// Convenience alias used throughout the command layer.
pub type Result<T> = std::result::Result<T, SafaiError>;

//! Domain errors. Library-style: precise enum via `thiserror`.

use thiserror::Error;

/// All failures that pure domain logic can report.
#[derive(Debug, Error, PartialEq)]
pub enum DomainError {
    /// Hex color string is malformed (must be `#RRGGBB` or `RRGGBB`).
    #[error("invalid hex color: {0}")]
    InvalidHexColor(String),
    /// Alpha channel outside `0.0..=1.0`.
    #[error("alpha out of range (0.0..=1.0): {0}")]
    AlphaOutOfRange(f32),
    /// Bar count outside `1..=1024`.
    #[error("bar count out of range (1..=1024): {0}")]
    BarCountOutOfRange(u32),
    /// Framerate outside `1..=240`.
    #[error("framerate out of range (1..=240): {0}")]
    FramerateOutOfRange(u32),
    /// Bar gap ratio outside `0.0..=5.0`.
    #[error("bar gap out of range (0.0..=5.0): {0}")]
    GapOutOfRange(f32),
    /// Raw cava frame has an unexpected byte length.
    #[error("cava frame length mismatch: expected {expected} bytes, got {got}")]
    FrameLengthMismatch {
        /// Expected byte count (`bars * 2`).
        expected: usize,
        /// Actual byte count received.
        got: usize,
    },
    /// Level value count does not match the configured bar count.
    #[error("level count mismatch: expected {expected}, got {got}")]
    LevelCountMismatch {
        /// Expected bar count.
        expected: usize,
        /// Actual level count.
        got: usize,
    },
    /// Adapter (IO/process) failure behind an [`crate::ports::AudioSource`].
    ///
    /// Lives here so the port trait stays object-safe without leaking
    /// `std::io` types into pure logic; only adapters construct it.
    #[error("audio source failure: {0}")]
    Source(String),
}

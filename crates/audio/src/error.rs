//! Audio errors: child-process, decode, and config failures.

use thiserror::Error;

/// Failures from audio ingest (cava child, frame decoding).
#[derive(Debug, Error)]
pub enum AudioError {
    /// Config validation failure behind a cava operation.
    #[error(transparent)]
    Config(#[from] wallpaper_cava_config::ConfigError),
    /// cava child-process failure (spawn, pipe, config).
    #[error("cava: {0}")]
    Cava(String),
    /// Raw cava frame has an unexpected byte length.
    #[error("cava frame length mismatch: expected {expected} bytes, got {got}")]
    FrameLengthMismatch {
        /// Expected byte count (`bars * 2`).
        expected: usize,
        /// Actual byte count received.
        got: usize,
    },
    /// Spectrum source failure (pipe read behind [`crate::cava::CavaSource`]).
    #[error("audio source failure: {0}")]
    Source(String),
}

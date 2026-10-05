//! Config errors. Library-style: precise enum via `thiserror`.

use thiserror::Error;

/// All failures that config parsing and validation can report.
#[derive(Debug, Error, PartialEq)]
pub enum ConfigError {
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
}

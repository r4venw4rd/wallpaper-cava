//! Wallpaper configuration: file shape, validated newtypes, parsing.
//!
//! Pure types only: no IO, no audio, no rendering. Constructors reject
//! illegal states so the rest of the codebase never checks ranges again.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(missing_docs)]
#![warn(clippy::todo)]
#![warn(clippy::pedantic)]

pub mod config;
pub mod error;

pub use config::{
    BarConfig, BarCount, Config, ConfigColor, Framerate, GapRatio, GeneralConfig, HexColorConfig,
    OutputConfig, Rgba, SmoothingConfig,
};
pub use error::ConfigError;

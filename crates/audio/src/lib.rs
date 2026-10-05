//! Audio ingest: cava child process, frame decoding, resampling.
//!
//! Spawns `cava` for DSP, decodes its `raw / 16bit` pipe, and resamples
//! frames to per-view bar counts. No rendering here.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(missing_docs)]
#![warn(clippy::todo)]
#![warn(clippy::pedantic)]

pub mod bars;
pub mod cava;
pub mod decode;
pub mod error;

pub use bars::cava_bars;
pub use cava::{cava_config_toml, CavaSource};
pub use decode::{parse_cava_frame, resample_levels};
pub use error::AudioError;

//! Pure domain core: validated types and stateless calculations.
//!
//! No IO, no Wayland, no EGL here. All functions are synchronous.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(missing_docs)]
#![warn(clippy::todo)]
#![warn(clippy::pedantic)]

pub mod audio;
pub mod color;
pub mod config;
pub mod error;
pub mod geometry;
pub mod outputs;
pub mod ports;

pub use audio::{parse_cava_frame, resample_levels};
pub use color::{array_from_config_color, gradient_ssbo_bytes, hex_to_rgba, resolve_gradient};
pub use config::{
    BarConfig, BarCount, Config, ConfigColor, Framerate, GapRatio, GeneralConfig, HexColorConfig,
    OutputConfig, Rgba, SmoothingConfig,
};
pub use error::DomainError;
pub use geometry::{indices_for_bars, vertices_for_levels};
pub use outputs::{cava_bars, resolve_output, ResolvedOutput};
pub use ports::AudioSource;

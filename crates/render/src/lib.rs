//! Rendering: bar geometry, gradients, and the Wayland wallpaper shell.
//!
//! Pure layout math (`geometry`, `color`, `outputs`) plus the platform
//! adapter (`shell`, `gl_util`). The only place with `unsafe` (every block
//! has a `// SAFETY:` note).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(missing_docs)]
#![warn(clippy::todo)]
#![warn(clippy::pedantic)]

pub mod color;
pub mod error;
pub mod geometry;
pub mod gl_util;
pub mod outputs;
pub mod shell;

pub use error::RenderError;
pub use shell::WallpaperShell;
/// Re-exported so the binary wires one crate: the source lives in audio.
pub use wallpaper_cava_audio::CavaSource;

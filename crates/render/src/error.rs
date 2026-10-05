//! Render errors: GL/EGL/Wayland plus upstream audio/config failures.

use thiserror::Error;

/// Failures from rendering (geometry, GL, Wayland shell).
#[derive(Debug, Error)]
pub enum RenderError {
    /// Config validation failure behind a render operation.
    #[error(transparent)]
    Config(#[from] wallpaper_cava_config::ConfigError),
    /// Audio failure behind a frame draw.
    #[error(transparent)]
    Audio(#[from] wallpaper_cava_audio::AudioError),
    /// Underlying OS IO failure.
    #[error("io: {0}")]
    Io(String),
    /// EGL failure.
    #[error("egl: {0}")]
    Egl(String),
    /// OpenGL failure (shader compile/link, loader, state).
    #[error("gl: {0}")]
    Gl(String),
    /// Wayland protocol / toolkit failure.
    #[error("wayland: {0}")]
    Wayland(String),
    /// Level value count does not match the configured bar count.
    #[error("level count mismatch: expected {expected}, got {got}")]
    LevelCountMismatch {
        /// Expected bar count.
        expected: usize,
        /// Actual level count.
        got: usize,
    },
}

impl From<std::io::Error> for RenderError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

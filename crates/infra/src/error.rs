//! Adapter error type. Binary-style precision is unnecessary here —
//! adapters translate platform failures into structured variants.

use thiserror::Error;

/// Failures from IO / FFI adapters.
#[derive(Debug, Error)]
pub enum InfraError {
    /// Pure-logic failure bubbled up through an adapter.
    #[error(transparent)]
    Domain(#[from] wallpaper_cava_domain::DomainError),
    /// Underlying OS IO failure.
    #[error("io: {0}")]
    Io(String),
    /// cava child-process failure (spawn, pipe, config).
    #[error("cava: {0}")]
    Cava(String),
    /// EGL failure.
    #[error("egl: {0}")]
    Egl(String),
    /// OpenGL failure (shader compile/link, loader, state).
    #[error("gl: {0}")]
    Gl(String),
    /// Wayland protocol / toolkit failure.
    #[error("wayland: {0}")]
    Wayland(String),
    /// Config file could not be read or understood.
    #[error("config: {0}")]
    Config(String),
}

impl From<std::io::Error> for InfraError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

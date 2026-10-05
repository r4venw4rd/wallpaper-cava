//! Ports (interfaces) owned by the domain.
//!
//! Concrete adapters live in `infra`; the binary injects them. Only one
//! seam exists on purpose: the audio source. Everything else is either pure
//! (tested directly) or a thin platform binding.

use crate::error::DomainError;

/// Synchronous spectrum source (e.g. a cava child process).
///
/// Kept synchronous on purpose: frame reads are small, blocking, and driven
/// by the Wayland frame callback, not by an async executor.
pub trait AudioSource {
    /// Block until the next frame arrives and return normalized levels.
    ///
    /// # Errors
    /// Returns [`DomainError`] when the frame cannot be read or decoded.
    fn next_levels(&mut self) -> Result<Vec<f32>, DomainError>;
}

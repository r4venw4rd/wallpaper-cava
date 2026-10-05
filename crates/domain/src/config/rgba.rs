//! Linear-space RGBA color, channels in `0.0..=1.0`.

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// Linear-space RGBA color, channels in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rgba {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

impl Rgba {
    /// Build a color; all channels must be in `0.0..=1.0`.
    ///
    /// # Errors
    /// Returns [`DomainError::AlphaOutOfRange`] when `a` is outside range.
    /// `r`, `g`, `b` come from trusted hex decoding, so only `a` is checked.
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Result<Self, DomainError> {
        if !(0.0..=1.0).contains(&a) {
            return Err(DomainError::AlphaOutOfRange(a));
        }
        Ok(Self { r, g, b, a })
    }

    /// Red channel.
    #[must_use]
    pub fn r(self) -> f32 {
        self.r
    }

    /// Green channel.
    #[must_use]
    pub fn g(self) -> f32 {
        self.g
    }

    /// Blue channel.
    #[must_use]
    pub fn b(self) -> f32 {
        self.b
    }

    /// Alpha channel.
    #[must_use]
    pub fn a(self) -> f32 {
        self.a
    }

    /// Channels as `[r, g, b, a]`.
    #[must_use]
    pub fn as_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn rejects_alpha_above_one() {
        assert!(Rgba::new(1.0, 1.0, 1.0, 1.5).is_err());
    }
}

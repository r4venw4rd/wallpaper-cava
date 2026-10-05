//! Validated scalar newtypes: bar count, framerate, gap ratio.

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// Number of spectrum bars (`1..=1024`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarCount(u32);

impl BarCount {
    /// Maximum supported bar count.
    pub const MAX: u32 = 1024;

    /// Build a validated bar count.
    ///
    /// # Errors
    /// Returns [`DomainError::BarCountOutOfRange`] when `value` is `0` or
    /// above [`BarCount::MAX`].
    pub fn new(value: u32) -> Result<Self, DomainError> {
        if value == 0 || value > Self::MAX {
            return Err(DomainError::BarCountOutOfRange(value));
        }
        Ok(Self(value))
    }

    /// Raw value.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

/// Target frames per second (`1..=240`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Framerate(u32);

impl Framerate {
    /// Build a validated framerate.
    ///
    /// # Errors
    /// Returns [`DomainError::FramerateOutOfRange`] when outside `1..=240`.
    pub fn new(value: u32) -> Result<Self, DomainError> {
        if value == 0 || value > 240 {
            return Err(DomainError::FramerateOutOfRange(value));
        }
        Ok(Self(value))
    }

    /// Raw value.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

/// Gap width expressed as a fraction of one bar width (`0.0..=5.0`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GapRatio(f32);

impl GapRatio {
    /// Build a validated gap ratio.
    ///
    /// # Errors
    /// Returns [`DomainError::GapOutOfRange`] for negative values, values
    /// above `5.0`, or non-finite floats.
    pub fn new(value: f32) -> Result<Self, DomainError> {
        if !(0.0..=5.0).contains(&value) {
            return Err(DomainError::GapOutOfRange(value));
        }
        Ok(Self(value))
    }

    /// Raw value.
    #[must_use]
    pub fn get(self) -> f32 {
        self.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_bar_count() {
        assert!(BarCount::new(0).is_err());
    }

    #[test]
    fn rejects_zero_framerate() {
        assert!(Framerate::new(0).is_err());
    }

    #[test]
    fn rejects_negative_gap() {
        assert!(GapRatio::new(-0.5).is_err());
    }

    #[test]
    fn rejects_nan_gap() {
        assert!(GapRatio::new(f32::NAN).is_err());
    }
}

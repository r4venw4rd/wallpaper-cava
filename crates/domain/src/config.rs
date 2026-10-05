//! Validated configuration newtypes and settings structs.
//!
//! Parse, don't validate: constructors reject illegal states so the rest of
//! the codebase never checks ranges again.

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

/// Single color entry in the config file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigColor {
    /// Bare `"#RRGGBB"` string, alpha assumed `1.0`.
    Simple(String),
    /// `{ hex = "#RRGGBB", alpha = 0.9 }` table.
    Complex(HexColorConfig),
}

/// Hex + optional alpha table form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HexColorConfig {
    /// Hex string, `"#RRGGBB"` (or legacy 8-digit `#RRGGBBAA`-ish forms,
    /// see [`crate::color::hex_to_rgba`]).
    pub hex: String,
    /// Optional alpha override; defaults to `1.0`.
    pub alpha: Option<f32>,
}

/// `[general]` section of the wallpaper config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Target framerate.
    pub framerate: u32,
    /// Clear color behind the bars.
    pub background_color: ConfigColor,
    /// Cava `autosens` passthrough.
    pub autosens: Option<bool>,
    /// Cava `sensitivity` passthrough.
    pub sensitivity: Option<f32>,
    /// Preferred Wayland output name (e.g. `"DP-1"`).
    pub preferred_output: Option<String>,
}

/// `[bars]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarConfig {
    /// Bar count.
    pub amount: u32,
    /// Gap ratio.
    pub gap: f32,
}

/// `[smoothing]` section, passed straight through to cava.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmoothingConfig {
    /// Monstercat smoothing factor.
    pub monstercat: Option<f32>,
    /// Waves smoothing.
    pub waves: Option<i32>,
    /// Noise reduction factor.
    pub noise_reduction: Option<f32>,
}

/// Root wallpaper configuration (file shape, unvalidated).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// General section.
    pub general: GeneralConfig,
    /// Bars section.
    pub bars: BarConfig,
    /// Gradient stops, keyed `gradient_color_N`.
    pub colors: std::collections::HashMap<String, ConfigColor>,
    /// Smoothing section.
    pub smoothing: SmoothingConfig,
}

impl Config {
    /// Parse a TOML config document.
    ///
    /// Shape errors (missing tables) surface as [`DomainError`] via a string
    /// message in [`DomainError::InvalidHexColor`]; range validation happens
    /// in [`Config::validated`].
    ///
    /// # Errors
    /// Returns an error when the document is not valid TOML with the
    /// expected shape.
    pub fn from_toml_str(s: &str) -> Result<Self, DomainError> {
        toml::from_str(s).map_err(|e| DomainError::InvalidHexColor(e.to_string()))
    }

    /// Validated bar count.
    ///
    /// # Errors
    /// Forwards [`BarCount::new`] range errors.
    pub fn bar_count(&self) -> Result<BarCount, DomainError> {
        BarCount::new(self.bars.amount)
    }

    /// Validated framerate.
    ///
    /// # Errors
    /// Forwards [`Framerate::new`] range errors.
    pub fn framerate(&self) -> Result<Framerate, DomainError> {
        Framerate::new(self.general.framerate)
    }

    /// Validated gap ratio.
    ///
    /// # Errors
    /// Forwards [`GapRatio::new`] range errors.
    pub fn gap_ratio(&self) -> Result<GapRatio, DomainError> {
        GapRatio::new(self.bars.gap)
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

    #[test]
    fn rejects_alpha_above_one() {
        assert!(Rgba::new(1.0, 1.0, 1.0, 1.5).is_err());
    }
}

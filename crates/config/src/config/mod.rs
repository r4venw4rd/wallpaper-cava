//! Validated configuration newtypes and settings structs.
//!
//! Parse, don't validate: constructors reject illegal states so the rest of
//! the codebase never checks ranges again.

pub mod counts;
pub mod rgba;
pub mod schema;

pub use counts::{BarCount, Framerate, GapRatio};
pub use rgba::Rgba;
pub use schema::{BarConfig, ConfigColor, GeneralConfig, HexColorConfig, SmoothingConfig};

use crate::error::ConfigError;

/// Root wallpaper configuration (file shape, unvalidated).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    /// General section.
    pub general: GeneralConfig,
    /// Bars section (global defaults).
    pub bars: BarConfig,
    /// Gradient stops, keyed `gradient_color_N` (global defaults).
    pub colors: std::collections::HashMap<String, ConfigColor>,
    /// Smoothing section.
    pub smoothing: SmoothingConfig,
    /// Per-output overrides, keyed by Wayland output name
    /// (`[outputs."DP-1"]`). Absent for old configs.
    #[serde(default)]
    pub outputs: std::collections::HashMap<String, OutputConfig>,
}

/// Per-output overrides (`[outputs."DP-1"]`). Every field is optional;
///
/// anything unset falls back to the global sections.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct OutputConfig {
    /// Clear color override.
    pub background_color: Option<ConfigColor>,
    /// Bar count override.
    pub bars_amount: Option<u32>,
    /// Gap ratio override.
    pub bars_gap: Option<f32>,
    /// Gradient stops override, keyed `gradient_color_N`.
    pub colors: Option<std::collections::HashMap<String, ConfigColor>>,
}

impl Config {
    /// Parse a TOML config document.
    ///
    /// Shape errors (missing tables) surface as [`ConfigError`] via a string
    /// message in [`ConfigError::InvalidHexColor`]; range validation happens
    /// in [`Config::validated`].
    ///
    /// # Errors
    /// Returns an error when the document is not valid TOML with the
    /// expected shape.
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        toml::from_str(s).map_err(|e| ConfigError::InvalidHexColor(e.to_string()))
    }

    /// Validated bar count.
    ///
    /// # Errors
    /// Forwards [`BarCount::new`] range errors.
    pub fn bar_count(&self) -> Result<BarCount, ConfigError> {
        BarCount::new(self.bars.amount)
    }

    /// Validated framerate.
    ///
    /// # Errors
    /// Forwards [`Framerate::new`] range errors.
    pub fn framerate(&self) -> Result<Framerate, ConfigError> {
        Framerate::new(self.general.framerate)
    }

    /// Validated gap ratio.
    ///
    /// # Errors
    /// Forwards [`GapRatio::new`] range errors.
    pub fn gap_ratio(&self) -> Result<GapRatio, ConfigError> {
        GapRatio::new(self.bars.gap)
    }
}

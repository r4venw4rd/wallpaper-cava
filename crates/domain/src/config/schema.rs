//! Config file sections: general, bars, smoothing, color entries.

use serde::{Deserialize, Serialize};

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

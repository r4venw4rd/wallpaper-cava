//! Generated cava config: validated render of the passthrough subset.

use std::collections::HashMap;

use wallpaper_cava_config::{BarCount, Config, Framerate};

use crate::error::AudioError;

/// Serializable cava `[general]` section (passthrough subset).
#[derive(Debug, serde::Serialize)]
struct CavaGeneral {
    /// Frames per second.
    framerate: u32,
    /// Bar count.
    bars: u32,
    /// Autosens passthrough.
    autosens: Option<bool>,
    /// Sensitivity passthrough.
    sensitivity: Option<f32>,
}

/// Serializable cava `[smoothing]` section.
#[derive(Debug, serde::Serialize)]
struct CavaSmoothing {
    /// Monstercat factor.
    monstercat: Option<f32>,
    /// Waves factor.
    waves: Option<i32>,
    /// Noise reduction factor.
    noise_reduction: Option<f32>,
}

/// Full generated cava config.
#[derive(Debug, serde::Serialize)]
struct CavaConfig {
    /// General section.
    general: CavaGeneral,
    /// Smoothing section.
    smoothing: CavaSmoothing,
    /// Output section (`raw` to stdout).
    output: HashMap<String, String>,
}

/// Render the generated cava config, running at `bars` (the max over all
/// views — views with fewer bars get a resampled copy).
///
/// Re-validates the values passed straight through to cava: the binary calls
/// [`Config::framerate`] before spawning, but this constructor must not trust
/// its caller — an unvalidated `0` or non-finite float would otherwise reach
/// the child process config.
///
/// # Errors
/// Returns [`AudioError`] when a value is out of range, non-finite, or TOML
/// serialization fails.
pub fn cava_config_toml(config: &Config, bars: BarCount) -> Result<String, AudioError> {
    let framerate = Framerate::new(config.general.framerate)?;
    let output: HashMap<String, String> = HashMap::from([
        ("method".to_string(), "raw".to_string()),
        ("raw_target".to_string(), "/dev/stdout".to_string()),
        ("bit_format".to_string(), "16bit".to_string()),
    ]);
    let cava = CavaConfig {
        general: CavaGeneral {
            framerate: framerate.get(),
            bars: bars.get(),
            autosens: config.general.autosens,
            sensitivity: require_finite(config.general.sensitivity, "sensitivity")?,
        },
        smoothing: CavaSmoothing {
            monstercat: require_finite(config.smoothing.monstercat, "monstercat")?,
            waves: config.smoothing.waves,
            noise_reduction: require_finite(config.smoothing.noise_reduction, "noise_reduction")?,
        },
        output,
    };
    toml::to_string(&cava).map_err(|e| AudioError::Cava(e.to_string()))
}

/// Reject non-finite passthrough floats before they reach the cava config.
///
/// TOML accepts `inf`/`nan` literals, and `toml::to_string` would fail on
/// them later with a generic error — fail here with the field name instead.
fn require_finite(value: Option<f32>, name: &str) -> Result<Option<f32>, AudioError> {
    match value {
        Some(v) if !v.is_finite() => Err(AudioError::Cava(format!("{name} must be finite"))),
        other => Ok(other),
    }
}

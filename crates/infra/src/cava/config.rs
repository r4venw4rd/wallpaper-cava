//! Generated cava config: validated render of the passthrough subset.

use std::collections::HashMap;

use wallpaper_cava_domain::{BarCount, Config};

use crate::error::InfraError;

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
/// # Errors
/// Returns [`InfraError::Cava`] when TOML serialization fails.
pub fn cava_config_toml(config: &Config, bars: BarCount) -> Result<String, InfraError> {
    let output: HashMap<String, String> = HashMap::from([
        ("method".to_string(), "raw".to_string()),
        ("raw_target".to_string(), "/dev/stdout".to_string()),
        ("bit_format".to_string(), "16bit".to_string()),
    ]);
    let cava = CavaConfig {
        general: CavaGeneral {
            framerate: config.general.framerate,
            bars: bars.get(),
            autosens: config.general.autosens,
            sensitivity: config.general.sensitivity,
        },
        smoothing: CavaSmoothing {
            monstercat: config.smoothing.monstercat,
            waves: config.smoothing.waves,
            noise_reduction: config.smoothing.noise_reduction,
        },
        output,
    };
    toml::to_string(&cava).map_err(|e| InfraError::Cava(e.to_string()))
}

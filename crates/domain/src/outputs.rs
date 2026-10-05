//! Per-output resolution: merge `[outputs."NAME"]` overrides onto globals.
//!
//! Pure and total: every override is validated, missing pieces fall back
//! to the global sections.

use std::collections::HashMap;

use crate::color::{array_from_config_color, resolve_gradient};
use crate::config::{BarCount, Config, GapRatio, OutputConfig, Rgba};
use crate::error::DomainError;

/// Fully resolved render settings for one output (or the global default).
#[derive(Debug, Clone)]
pub struct ResolvedOutput {
    /// Bar count for this output.
    pub bars: BarCount,
    /// Gap ratio for this output.
    pub gap: GapRatio,
    /// Clear color for this output.
    pub background: Rgba,
    /// Gradient stops for this output (sorted, non-empty).
    pub gradient: Vec<Rgba>,
}

/// Resolve settings for `name` (`None` = global default).
///
/// # Examples
///
/// ```
/// use wallpaper_cava_domain::config::Config;
/// use wallpaper_cava_domain::outputs::resolve_output;
/// let cfg: Config = toml::from_str(
///     "[general]\nframerate = 60\nbackground_color = '#000000'\n\
///      [bars]\namount = 4\ngap = 0.1\n\
///      [colors]\ngradient_color_1 = '#ffffff'\n\
///      [smoothing]\n",
/// )
/// .unwrap();
/// let out = resolve_output(None, &cfg).unwrap();
/// assert_eq!(out.bars.get(), 4);
/// ```
///
/// # Errors
/// Returns [`DomainError`] on malformed colors or out-of-range overrides.
pub fn resolve_output(name: Option<&str>, config: &Config) -> Result<ResolvedOutput, DomainError> {
    let overlay: Option<&OutputConfig> = name.and_then(|n| config.outputs.get(n));
    let bars = BarCount::new(
        overlay
            .and_then(|o| o.bars_amount)
            .unwrap_or(config.bars.amount),
    )?;
    let gap = GapRatio::new(overlay.and_then(|o| o.bars_gap).unwrap_or(config.bars.gap))?;
    let background = overlay
        .and_then(|o| o.background_color.as_ref())
        .unwrap_or(&config.general.background_color);
    let background = array_from_config_color(background)?;
    let gradient_map: HashMap<String, crate::config::ConfigColor> = overlay
        .and_then(|o| o.colors.clone())
        .unwrap_or_else(|| config.colors.clone());
    let gradient = resolve_gradient(&gradient_map)?;
    Ok(ResolvedOutput {
        bars,
        gap,
        background,
        gradient,
    })
}

/// Bar count cava must run at: the max over default + all outputs.
///
/// A single cava process feeds every view; views with fewer bars get a
/// resampled copy (see [`crate::audio::resample_levels`]).
///
/// # Errors
/// Returns [`DomainError`] when any bar count is out of range.
pub fn cava_bars(config: &Config) -> Result<BarCount, DomainError> {
    let mut max = config.bars.amount;
    for output in config.outputs.values() {
        if let Some(amount) = output.bars_amount {
            max = max.max(amount);
        }
    }
    BarCount::new(max)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Minimal config with `n` global bars.
    fn test_config(bars: u32) -> Config {
        toml::from_str(&format!(
            "[general]\nframerate = 60\nbackground_color = '#000000'\n\
             [bars]\namount = {bars}\ngap = 0.1\n\
             [colors]\ngradient_color_1 = '#ffffff'\n\
             [smoothing]\n"
        ))
        .unwrap()
    }

    #[test]
    fn default_resolves_globals() {
        let out = resolve_output(None, &test_config(8)).unwrap();
        assert_eq!(out.bars.get(), 8);
    }

    #[test]
    fn output_override_wins() {
        let mut cfg = test_config(8);
        cfg.outputs.insert(
            "DP-1".to_string(),
            OutputConfig {
                bars_amount: Some(16),
                ..OutputConfig::default()
            },
        );
        assert_eq!(resolve_output(Some("DP-1"), &cfg).unwrap().bars.get(), 16);
        assert_eq!(resolve_output(Some("HDMI-1"), &cfg).unwrap().bars.get(), 8);
    }

    #[test]
    fn cava_runs_at_max_bars() {
        let mut cfg = test_config(8);
        cfg.outputs.insert(
            "DP-1".to_string(),
            OutputConfig {
                bars_amount: Some(32),
                ..OutputConfig::default()
            },
        );
        assert_eq!(cava_bars(&cfg).unwrap().get(), 32);
    }

    #[test]
    fn rejects_bad_override() {
        let mut cfg = test_config(8);
        cfg.outputs.insert(
            "DP-1".to_string(),
            OutputConfig {
                bars_amount: Some(0),
                ..OutputConfig::default()
            },
        );
        assert!(resolve_output(Some("DP-1"), &cfg).is_err());
    }
}

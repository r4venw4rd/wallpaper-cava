//! Cava bar budget: one child process feeds every view.

use wallpaper_cava_config::{BarCount, Config};

use crate::error::AudioError;

/// Bar count cava must run at: the max over default + all outputs.
///
/// A single cava process feeds every view; views with fewer bars get a
/// resampled copy (see [`crate::decode::resample_levels`]).
///
/// # Errors
/// Returns [`AudioError`] when any bar count is out of range.
pub fn cava_bars(config: &Config) -> Result<BarCount, AudioError> {
    let mut max = config.bars.amount;
    for output in config.outputs.values() {
        if let Some(amount) = output.bars_amount {
            max = max.max(amount);
        }
    }
    Ok(BarCount::new(max)?)
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
    fn cava_runs_at_max_bars() {
        use wallpaper_cava_config::OutputConfig;
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
}

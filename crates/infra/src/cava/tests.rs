//! Adapter tests: generated config validation (no cava binary needed).

#![allow(clippy::unwrap_used)]

use wallpaper_cava_domain::{BarCount, Config};

use super::config::cava_config_toml;

/// Minimal valid config with `framerate` and `sensitivity` overrides.
fn test_config(framerate: u32, sensitivity: &str) -> Config {
    toml::from_str(&format!(
        "[general]\nframerate = {framerate}\nbackground_color = '#000000'\n\
         sensitivity = {sensitivity}\n\
         [bars]\namount = 8\ngap = 0.1\n\
         [colors]\ngradient_color_1 = '#ffffff'\n\
         [smoothing]\n"
    ))
    .unwrap()
}

#[test]
fn rejects_zero_framerate() {
    let cfg = test_config(0, "40.0");
    assert!(cava_config_toml(&cfg, BarCount::new(8).unwrap()).is_err());
}

#[test]
fn rejects_nonfinite_sensitivity() {
    let cfg = test_config(60, "inf");
    assert!(cava_config_toml(&cfg, BarCount::new(8).unwrap()).is_err());
}

#[test]
fn accepts_valid_passthrough() {
    let cfg = test_config(60, "40.0");
    let rendered = cava_config_toml(&cfg, BarCount::new(8).unwrap()).unwrap();
    assert!(rendered.contains("bars = 8"));
}

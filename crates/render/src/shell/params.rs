//! Validated shell parameters: global default + per-output settings.

use std::collections::HashMap;

use wallpaper_cava_audio::cava_bars;
use wallpaper_cava_config::{BarCount, Config};

use crate::color::gradient_ssbo_bytes;
use crate::outputs::{resolve_output, ResolvedOutput};

use super::WallpaperShell;
use crate::error::RenderError;

/// Validated shell parameters (helper to keep [`WallpaperShell::create`]
/// honest about what it needs).
pub struct SelfParams {
    /// Bar count cava must run at (max over all views).
    pub cava_bars: BarCount,
    /// Global default settings.
    pub default: ResolvedOutput,
    /// Per-output settings, keyed by output name.
    pub outputs: HashMap<String, ResolvedOutput>,
    /// Preferred output name (single-view filter, kept for compatibility).
    pub preferred_output: Option<String>,
    /// Packed gradient SSBO bytes for the default (setup upload).
    pub gradient_bytes: Vec<u8>,
    /// Vertex shader source.
    pub vertex_src: String,
    /// Fragment shader source.
    pub fragment_src: String,
}

impl WallpaperShell {
    /// Collect validated render parameters from a [`Config`].
    ///
    /// Resolves the global default plus every `[outputs."NAME"]` section
    /// via pure domain logic.
    ///
    /// # Errors
    /// Returns [`RenderError`] on malformed colors or out-of-range values.
    pub fn collect_params(
        config: &Config,
        vertex_src: String,
        fragment_src: String,
    ) -> Result<SelfParams, RenderError> {
        let default = resolve_output(None, config)?;
        let mut outputs = HashMap::with_capacity(config.outputs.len());
        for name in config.outputs.keys() {
            outputs.insert(name.clone(), resolve_output(Some(name), config)?);
        }
        let cava_bars = cava_bars(config)?;
        let gradient_bytes = gradient_ssbo_bytes(&default.gradient);
        Ok(SelfParams {
            cava_bars,
            default,
            outputs,
            preferred_output: config.general.preferred_output.clone(),
            gradient_bytes,
            vertex_src,
            fragment_src,
        })
    }
}

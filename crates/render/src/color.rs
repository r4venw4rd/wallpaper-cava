//! Color parsing and gradient buffer layout (pure, no IO).

use wallpaper_cava_config::{ConfigColor, ConfigError, Rgba};

/// Decode one `RR` hex pair at `offset` (offset into the `#`-stripped body).
fn hex_pair(body: &str, offset: usize) -> Result<f32, ConfigError> {
    let pair = body
        .get(offset..offset + 2)
        .ok_or_else(|| ConfigError::InvalidHexColor(body.to_string()))?;
    let byte =
        u8::from_str_radix(pair, 16).map_err(|_| ConfigError::InvalidHexColor(body.to_string()))?;
    Ok(f32::from(byte) / 255.0)
}

/// Parse `#RRGGBB` (or bare `RRGGBB`) into an [`Rgba`].
///
/// Legacy 8-digit forms (`#RRGGBBAA`-ish, as found in some user configs) are
/// accepted by reading only the first six digits; the explicit `alpha`
/// argument wins. This keeps old configs rendering instead of failing.
///
/// # Examples
///
/// ```
/// use wallpaper_cava_render::color::hex_to_rgba;
/// let c = hex_to_rgba("#ff0000", 1.0).unwrap();
/// assert_eq!(c.as_array(), [1.0, 0.0, 0.0, 1.0]);
/// ```
///
/// # Errors
/// Returns [`ConfigError::InvalidHexColor`] for malformed strings and
/// [`ConfigError::AlphaOutOfRange`] for bad alpha.
pub fn hex_to_rgba(hex: &str, alpha: f32) -> Result<Rgba, ConfigError> {
    let body = hex.strip_prefix('#').unwrap_or(hex);
    if body.len() != 6 && body.len() != 8 {
        return Err(ConfigError::InvalidHexColor(hex.to_string()));
    }
    let r = hex_pair(body, 0)?;
    let g = hex_pair(body, 2)?;
    let b = hex_pair(body, 4)?;
    Rgba::new(r, g, b, alpha)
}

/// Convert a [`ConfigColor`] entry into an [`Rgba`].
///
/// # Errors
/// Forwards [`hex_to_rgba`] errors; missing alpha defaults to `1.0`.
pub fn array_from_config_color(color: &ConfigColor) -> Result<Rgba, ConfigError> {
    match color {
        ConfigColor::Simple(hex) => hex_to_rgba(hex, 1.0),
        ConfigColor::Complex(cfg) => hex_to_rgba(&cfg.hex, cfg.alpha.unwrap_or(1.0)),
    }
}

/// Resolve a gradient stop map into sorted [`Rgba`] stops.
///
/// Keys sort lexicographically, so `gradient_color_10` cannot silently
/// land before `gradient_color_2` (`HashMap` order is random).
///
/// # Errors
/// Returns [`ConfigError`] when the map is empty or any stop is malformed.
pub fn resolve_gradient<S: std::hash::BuildHasher>(
    stops: &std::collections::HashMap<String, ConfigColor, S>,
) -> Result<Vec<Rgba>, ConfigError> {
    let mut sorted: Vec<(&String, &ConfigColor)> = stops.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = Vec::with_capacity(sorted.len());
    for (_, color) in sorted {
        out.push(array_from_config_color(color)?);
    }
    if out.is_empty() {
        return Err(ConfigError::InvalidHexColor(
            "at least one gradient stop is required".to_string(),
        ));
    }
    Ok(out)
}

/// Pack gradient stops into the SSBO byte layout the fragment shader expects:
/// little-endian `int count`, 12 padding bytes (std430 `vec4` alignment),
/// then packed `vec4` colors.
///
/// # Examples
///
/// ```
/// use wallpaper_cava_render::color::{gradient_ssbo_bytes, hex_to_rgba};
/// let stops = [hex_to_rgba("#ffffff", 1.0).unwrap()];
/// let bytes = gradient_ssbo_bytes(&stops);
/// assert_eq!(bytes.len(), 16 + 16);
/// ```
#[must_use]
pub fn gradient_ssbo_bytes(colors: &[Rgba]) -> Vec<u8> {
    let count = i32::try_from(colors.len()).unwrap_or(i32::MAX);
    let mut out = Vec::with_capacity(16 + colors.len() * 16);
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&[0_u8; 12]);
    for c in colors {
        for v in c.as_array() {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn parses_mint_teal() {
        let c = hex_to_rgba("#94e2d5", 0.85).unwrap();
        assert!((c.r() - f32::from(0x94_u8) / 255.0).abs() < 1e-6);
        assert!((c.a() - 0.85).abs() < 1e-6);
    }

    #[test]
    fn rejects_short_hex() {
        assert!(hex_to_rgba("#fff", 1.0).is_err());
    }

    #[test]
    fn tolerates_legacy_8digit_hex() {
        assert!(hex_to_rgba("#20000077", 1.0).is_ok());
    }

    proptest! {
        #[test]
        fn ssbo_byte_len_matches_stop_count(n in 1usize..64) {
            let stops = vec![Rgba::new(1.0, 0.0, 0.0, 1.0).unwrap(); n];
            prop_assert_eq!(gradient_ssbo_bytes(&stops).len(), 16 + n * 16);
        }

        #[test]
        fn hex_roundtrip_is_stable(r in 0u8..=255, g in 0u8..=255, b in 0u8..=255) {
            let s = format!("#{r:02x}{g:02x}{b:02x}");
            let c = hex_to_rgba(&s, 1.0).unwrap();
            prop_assert!((c.r() - f32::from(r) / 255.0).abs() < 1e-6);
            prop_assert!((c.g() - f32::from(g) / 255.0).abs() < 1e-6);
            prop_assert!((c.b() - f32::from(b) / 255.0).abs() < 1e-6);
        }
    }
}

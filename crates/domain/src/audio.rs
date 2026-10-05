//! Raw cava frame decoding (pure).
//!
//! Cava streams little-endian `u16` magnitudes, one per bar. The original
//! client normalizes with the cava-specific divisor `65530.0`.

use crate::config::BarCount;
use crate::error::DomainError;

/// Divisor used by the original client to normalize raw `u16` samples.
pub const CAVA_NORMALIZER: f32 = 65530.0;

/// Decode one raw frame (`bars * 2` bytes) into `0.0..=1.0` levels.
///
/// # Examples
///
/// ```
/// use wallpaper_cava_domain::audio::parse_cava_frame;
/// use wallpaper_cava_domain::config::BarCount;
/// let raw = 32765u16.to_le_bytes();
/// let levels = parse_cava_frame(&raw, BarCount::new(1).unwrap()).unwrap();
/// assert!((levels[0] - 0.5).abs() < 0.01);
/// ```
///
/// # Errors
/// Returns [`DomainError::FrameLengthMismatch`] when `bytes.len()` is not
/// exactly `bars * 2`.
pub fn parse_cava_frame(bytes: &[u8], bars: BarCount) -> Result<Vec<f32>, DomainError> {
    let expected = bars.get() as usize * 2;
    if bytes.len() != expected {
        return Err(DomainError::FrameLengthMismatch {
            expected,
            got: bytes.len(),
        });
    }
    let mut out = Vec::with_capacity(bars.get() as usize);
    for chunk in bytes.chunks_exact(2) {
        let sample = u16::from_le_bytes([chunk[0], chunk[1]]);
        out.push((f32::from(sample) / CAVA_NORMALIZER).clamp(0.0, 1.0));
    }
    Ok(out)
}

/// Resample one frame to `target` bars.
///
/// Same count returns the input unchanged. Fewer bars average-pool
/// proportional buckets; more bars linearly interpolate. Output always
/// has exactly `target` values in `0.0..=1.0`.
///
/// # Examples
///
/// ```
/// use wallpaper_cava_domain::audio::resample_levels;
/// assert_eq!(resample_levels(&[0.0, 1.0], 2), vec![0.0, 1.0]);
/// assert_eq!(resample_levels(&[0.0, 0.0, 1.0, 1.0], 2), vec![0.0, 1.0]);
/// ```
#[must_use]
#[allow(clippy::cast_precision_loss)] // indices < 2^24 in practice; ratios only.
pub fn resample_levels(levels: &[f32], target: usize) -> Vec<f32> {
    if target == 0 {
        return Vec::new();
    }
    if levels.is_empty() {
        return vec![0.0; target];
    }
    if levels.len() == target {
        return levels.to_vec();
    }
    let src = levels.len();
    if target < src {
        // Average-pooling: bucket `i` covers [i*src/target, (i+1)*src/target).
        (0..target)
            .map(|i| {
                let start = i * src / target;
                let end = (i + 1) * src / target;
                let slice = &levels[start..end.max(start + 1).min(src)];
                slice.iter().sum::<f32>() / slice.len() as f32
            })
            .collect()
    } else {
        // Linear interpolation between source samples. `lo` uses integer
        // math: floor(i * (src-1) / target).
        (0..target)
            .map(|i| {
                let num = i * (src - 1);
                let lo = num / target;
                let hi = (lo + 1).min(src - 1);
                let frac = (num % target) as f32 / target as f32;
                levels[lo] * (1.0 - frac) + levels[hi] * frac
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rejects_short_frame() {
        assert!(parse_cava_frame(&[0_u8; 3], BarCount::new(2).unwrap()).is_err());
    }

    #[test]
    fn silence_decodes_to_zero() {
        let levels = parse_cava_frame(&[0_u8; 4], BarCount::new(2).unwrap()).unwrap();
        assert_eq!(levels, vec![0.0, 0.0]);
    }

    proptest! {
        #[test]
        fn levels_always_in_unit_range(samples in proptest::collection::vec(0u16..=u16::MAX, 1..=64)) {
            let bars = BarCount::new(u32::try_from(samples.len()).unwrap_or(64)).unwrap();
            let mut bytes = Vec::with_capacity(samples.len() * 2);
            for s in &samples {
                bytes.extend_from_slice(&s.to_le_bytes());
            }
            let levels = parse_cava_frame(&bytes, bars).unwrap();
            prop_assert!(levels.iter().all(|l| (0.0..=1.0).contains(l)));
        }

        #[test]
        fn resample_output_len_matches_target(
            levels in proptest::collection::vec(0.0f32..=1.0, 1..=64),
            target in 1usize..=64,
        ) {
            prop_assert_eq!(resample_levels(&levels, target).len(), target);
        }

        #[test]
        fn resample_stays_in_unit_range(
            levels in proptest::collection::vec(0.0f32..=1.0, 1..=64),
            target in 1usize..=64,
        ) {
            prop_assert!(resample_levels(&levels, target)
                .iter()
                .all(|l| (0.0..=1.0).contains(l)));
        }
    }
}

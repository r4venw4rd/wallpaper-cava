//! Bar layout math: NDC vertex positions and element indices (pure).

use crate::error::RenderError;
use wallpaper_cava_config::{BarCount, GapRatio};

/// Width of one bar and one gap in NDC units (`-1.0..=1.0`).
///
/// Layout mirrors the original renderer: total width `2.0` is split into
/// `n` bars plus `n - 1` gaps, where `gap_width = bar_width * gap`.
///
/// # Examples
///
/// ```
/// use wallpaper_cava_config::{BarCount, GapRatio};
/// use wallpaper_cava_render::geometry::bar_widths;
/// let (w, g) = bar_widths(BarCount::new(2).unwrap(), GapRatio::new(0.0).unwrap());
/// assert!((w - 1.0).abs() < 1e-6);
/// assert_eq!(g, 0.0);
/// ```
#[must_use]
#[allow(clippy::cast_precision_loss)] // BarCount <= 1024 < 2^24: u32 -> f32 exact.
pub fn bar_widths(count: BarCount, gap: GapRatio) -> (f32, f32) {
    let n = count.get() as f32;
    let bar = 2.0 / (n + (n - 1.0) * gap.get());
    (bar, bar * gap.get())
}

/// Build the vertex buffer for one frame: 4 corners (`x, y` pairs) per bar.
///
/// `levels` holds one `0.0..=1.0` magnitude per bar; bar tops map to
/// `2.0 * level - 1.0` in NDC, bottoms sit at `-1.0`.
///
/// # Examples
///
/// ```
/// use wallpaper_cava_config::{BarCount, GapRatio};
/// use wallpaper_cava_render::geometry::vertices_for_levels;
/// let v = vertices_for_levels(&[1.0], BarCount::new(1).unwrap(), GapRatio::new(0.0).unwrap()).unwrap();
/// assert_eq!(v.len(), 8);
/// assert!((v[1] - 1.0).abs() < 1e-6); // full bar reaches the top
/// ```
///
/// # Errors
/// Returns [`RenderError::LevelCountMismatch`] when `levels.len()` differs
/// from the bar count.
#[allow(clippy::cast_precision_loss)] // loop index < BarCount <= 1024: exact.
pub fn vertices_for_levels(
    levels: &[f32],
    count: BarCount,
    gap: GapRatio,
) -> Result<Vec<f32>, RenderError> {
    let n = count.get() as usize;
    if levels.len() != n {
        return Err(RenderError::LevelCountMismatch {
            expected: n,
            got: levels.len(),
        });
    }
    let (bar_width, gap_width) = bar_widths(count, gap);
    let mut out = vec![0.0_f32; n * 8];
    for (i, level) in levels.iter().enumerate() {
        let clamped = level.clamp(0.0, 1.0);
        let top = 2.0 * clamped - 1.0;
        let x0 = gap_width * i as f32 + bar_width * i as f32 - 1.0;
        let x1 = gap_width * i as f32 + bar_width * (i + 1) as f32 - 1.0;
        let o = i * 8;
        out[o] = x0;
        out[o + 1] = top;
        out[o + 2] = x1;
        out[o + 3] = top;
        out[o + 4] = x0;
        out[o + 5] = -1.0;
        out[o + 6] = x1;
        out[o + 7] = -1.0;
    }
    Ok(out)
}

/// Build the element index buffer: two triangles (6 indices) per bar.
///
/// # Examples
///
/// ```
/// use wallpaper_cava_config::BarCount;
/// use wallpaper_cava_render::geometry::indices_for_bars;
/// assert_eq!(indices_for_bars(BarCount::new(1).unwrap()), vec![0, 1, 2, 1, 2, 3]);
/// ```
#[must_use]
#[allow(clippy::cast_possible_truncation)] // index < BarCount <= 1024: no truncation.
pub fn indices_for_bars(count: BarCount) -> Vec<u16> {
    let n = count.get() as usize;
    let mut out = Vec::with_capacity(n * 6);
    for i in 0..n {
        let base = i as u16 * 4;
        out.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 2, base + 3]);
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn single_full_bar_spans_ndc() {
        let v = vertices_for_levels(
            &[1.0],
            BarCount::new(1).unwrap(),
            GapRatio::new(0.0).unwrap(),
        )
        .unwrap();
        assert_eq!(v, vec![-1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0]);
    }

    #[test]
    fn rejects_level_count_mismatch() {
        let r = vertices_for_levels(
            &[0.5, 0.5],
            BarCount::new(1).unwrap(),
            GapRatio::new(0.0).unwrap(),
        );
        assert!(r.is_err());
    }

    proptest! {
        #[test]
        fn vertex_buffer_len_matches_bars(n in 1u32..128) {
            let count = BarCount::new(n).unwrap();
            let levels = vec![0.5_f32; n as usize];
            let v = vertices_for_levels(&levels, count, GapRatio::new(0.15).unwrap()).unwrap();
            prop_assert_eq!(v.len(), n as usize * 8);
        }

        #[test]
        fn bar_tops_stay_in_ndc(level in 0.0f32..=1.0) {
            let v = vertices_for_levels(
                &[level],
                BarCount::new(1).unwrap(),
                GapRatio::new(0.0).unwrap(),
            )
            .unwrap();
            prop_assert!((-1.0..=1.0).contains(&v[1]));
        }
    }
}

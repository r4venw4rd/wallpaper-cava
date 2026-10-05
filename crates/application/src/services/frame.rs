//! Frame orchestration service.

use std::time::Duration;

use wallpaper_cava_domain::{vertices_for_levels, AudioSource, BarCount, DomainError, GapRatio};

/// Loop timing + vertex pipeline for one wallpaper instance.
///
/// Holds a borrowed [`AudioSource`] (dependency injection): production
/// passes the cava adapter, tests pass a stub — no IO is mocked.
pub struct FrameService<'a, S: AudioSource> {
    /// Injected spectrum source.
    source: &'a mut S,
    /// Validated bar count.
    bars: BarCount,
    /// Validated gap ratio.
    gap: GapRatio,
    /// Target frame rate.
    framerate: u32,
}

impl<'a, S: AudioSource> FrameService<'a, S> {
    /// Build the service. `framerate` is validated by the caller via
    /// [`wallpaper_cava_domain::Framerate`]; zero falls back to `60`.
    #[must_use]
    pub fn new(source: &'a mut S, bars: BarCount, gap: GapRatio, framerate: u32) -> Self {
        Self {
            source,
            bars,
            gap,
            framerate: framerate.max(1),
        }
    }

    /// Target time between frames.
    #[must_use]
    pub fn frame_duration(&self) -> Duration {
        Duration::from_secs(1) / self.framerate
    }

    /// Fetch the next levels and expand them into NDC vertices.
    ///
    /// # Errors
    /// Forwards source and geometry [`DomainError`]s unchanged.
    pub fn next_vertices(&mut self) -> Result<Vec<f32>, DomainError> {
        let levels = self.source.next_levels()?;
        vertices_for_levels(&levels, self.bars, self.gap)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use wallpaper_cava_domain::config::{BarCount, GapRatio};

    /// Deterministic stub: no cava, no pipe.
    struct StubSource {
        /// Levels to return.
        levels: Vec<f32>,
    }

    impl AudioSource for StubSource {
        fn next_levels(&mut self) -> Result<Vec<f32>, DomainError> {
            Ok(self.levels.clone())
        }
    }

    #[test]
    fn full_level_reaches_ndc_top() {
        let mut src = StubSource { levels: vec![1.0] };
        let mut svc = FrameService::new(
            &mut src,
            BarCount::new(1).unwrap(),
            GapRatio::new(0.0).unwrap(),
            60,
        );
        let v = svc.next_vertices().unwrap();
        assert!((v[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn frame_duration_matches_framerate() {
        let mut src = StubSource { levels: vec![0.0] };
        let svc = FrameService::new(
            &mut src,
            BarCount::new(1).unwrap(),
            GapRatio::new(0.0).unwrap(),
            60,
        );
        assert!(svc.frame_duration() <= Duration::from_millis(17));
    }
}

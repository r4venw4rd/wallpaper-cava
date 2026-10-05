//! Blocking spectrum source backed by a cava child process.

use std::io::{BufReader, Read, Write};
use std::process::{Child, ChildStdout, Command, Stdio};

use tracing::instrument;
use wallpaper_cava_config::{BarCount, Config};

use super::config::cava_config_toml;
use crate::decode::parse_cava_frame;
use crate::error::AudioError;

/// Blocking spectrum source backed by a cava child process.
pub struct CavaSource {
    /// cava stdout pipe.
    reader: BufReader<ChildStdout>,
    /// Configured bar count.
    bars: BarCount,
    /// Child handle; killed on drop.
    #[allow(dead_code)]
    child: Child,
}

impl CavaSource {
    /// Cava binary path: `$CAVA_BIN` when set and non-blank, else `cava`
    /// resolved from `PATH`. The env override pins an absolute path so a
    /// hostile `PATH` entry cannot redirect the spawn.
    #[must_use]
    pub fn binary() -> String {
        std::env::var("CAVA_BIN")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "cava".to_string())
    }

    /// Spawn `cava` running at `bars` — the max over all views, see
    /// [`cava_bars`](crate::bars::cava_bars).
    ///
    /// # Errors
    /// Returns [`AudioError`] when the binary is missing, pipes fail, or
    /// the generated config cannot be delivered.
    #[instrument(skip(config), fields(bars = bars.get()))]
    pub fn spawn(config: &Config, bars: BarCount) -> Result<Self, AudioError> {
        let toml = cava_config_toml(config, bars)?;
        let binary = Self::binary();
        tracing::info!(binary = %binary, "spawning cava");
        let mut child = Command::new(&binary)
            .arg("-p")
            .arg("/dev/stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| AudioError::Cava(format!("failed to spawn {binary}: {e}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AudioError::Cava("cava stdin unavailable".to_string()))?;
        let mut stdin = stdin;
        stdin
            .write_all(toml.as_bytes())
            .map_err(|e| AudioError::Cava(format!("failed to write cava config: {e}")))?;
        drop(stdin);
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AudioError::Cava("cava stdout unavailable".to_string()))?;
        Ok(Self {
            reader: BufReader::new(stdout),
            bars,
            child,
        })
    }
}

impl CavaSource {
    /// Block until the next frame arrives and return normalized levels.
    ///
    /// # Errors
    /// Returns [`AudioError`] when the frame cannot be read or decoded.
    pub fn next_levels(&mut self) -> Result<Vec<f32>, AudioError> {
        let expected = self.bars.get() as usize * 2;
        let mut buf = vec![0_u8; expected];
        self.reader
            .read_exact(&mut buf)
            .map_err(|e| AudioError::Source(e.to_string()))?;
        parse_cava_frame(&buf, self.bars)
    }
}

impl Drop for CavaSource {
    fn drop(&mut self) {
        if let Err(e) = self.child.kill() {
            // Already exited is fine; anything else is worth a line.
            tracing::debug!(error = %e, "cava child kill on drop");
        }
    }
}

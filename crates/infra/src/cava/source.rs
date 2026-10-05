//! Blocking spectrum source backed by a cava child process.

use std::io::{BufReader, Read, Write};
use std::process::{Child, ChildStdout, Command, Stdio};

use tracing::instrument;
use wallpaper_cava_domain::{parse_cava_frame, AudioSource, BarCount, Config, DomainError};

use super::config::cava_config_toml;
use crate::error::InfraError;

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
    /// Spawn `cava` running at `bars` (see [`cava_bars`]).
    ///
    /// [`cava_bars`]: wallpaper_cava_domain::cava_bars
    ///
    /// # Errors
    /// Returns [`InfraError`] when the binary is missing, pipes fail, or
    /// the generated config cannot be delivered.
    #[instrument(skip(config), fields(bars = bars.get()))]
    pub fn spawn(config: &Config, bars: BarCount) -> Result<Self, InfraError> {
        let toml = cava_config_toml(config, bars)?;
        let mut child = Command::new("cava")
            .arg("-p")
            .arg("/dev/stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| InfraError::Cava(format!("failed to spawn cava: {e}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| InfraError::Cava("cava stdin unavailable".to_string()))?;
        let mut stdin = stdin;
        stdin
            .write_all(toml.as_bytes())
            .map_err(|e| InfraError::Cava(format!("failed to write cava config: {e}")))?;
        drop(stdin);
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| InfraError::Cava("cava stdout unavailable".to_string()))?;
        Ok(Self {
            reader: BufReader::new(stdout),
            bars,
            child,
        })
    }
}

impl AudioSource for CavaSource {
    fn next_levels(&mut self) -> Result<Vec<f32>, DomainError> {
        let expected = self.bars.get() as usize * 2;
        let mut buf = vec![0_u8; expected];
        self.reader
            .read_exact(&mut buf)
            .map_err(|e| DomainError::Source(e.to_string()))?;
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

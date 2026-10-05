//! cava child-process adapter.
//!
//! The visualizer does not do DSP itself: it spawns `cava -p /dev/stdin`
//! with a generated `raw / 16bit` config and decodes the pipe with pure
//! domain logic.

use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::process::{Child, ChildStdout, Command, Stdio};

use tracing::instrument;
use wallpaper_cava_domain::{parse_cava_frame, AudioSource, BarCount, Config, DomainError};

use crate::error::InfraError;

/// Serializable cava `[general]` section (passthrough subset).
#[derive(Debug, serde::Serialize)]
struct CavaGeneral {
    /// Frames per second.
    framerate: u32,
    /// Bar count.
    bars: u32,
    /// Autosens passthrough.
    autosens: Option<bool>,
    /// Sensitivity passthrough.
    sensitivity: Option<f32>,
}

/// Serializable cava `[smoothing]` section.
#[derive(Debug, serde::Serialize)]
struct CavaSmoothing {
    /// Monstercat factor.
    monstercat: Option<f32>,
    /// Waves factor.
    waves: Option<i32>,
    /// Noise reduction factor.
    noise_reduction: Option<f32>,
}

/// Full generated cava config.
#[derive(Debug, serde::Serialize)]
struct CavaConfig {
    /// General section.
    general: CavaGeneral,
    /// Smoothing section.
    smoothing: CavaSmoothing,
    /// Output section (`raw` to stdout).
    output: HashMap<String, String>,
}

/// Render the generated cava config for `wallpaper` settings.
///
/// # Errors
/// Returns [`InfraError::Cava`] when TOML serialization fails.
pub fn cava_config_toml(config: &Config) -> Result<String, InfraError> {
    let output: HashMap<String, String> = HashMap::from([
        ("method".to_string(), "raw".to_string()),
        ("raw_target".to_string(), "/dev/stdout".to_string()),
        ("bit_format".to_string(), "16bit".to_string()),
    ]);
    let cava = CavaConfig {
        general: CavaGeneral {
            framerate: config.general.framerate,
            bars: config.bars.amount,
            autosens: config.general.autosens,
            sensitivity: config.general.sensitivity,
        },
        smoothing: CavaSmoothing {
            monstercat: config.smoothing.monstercat,
            waves: config.smoothing.waves,
            noise_reduction: config.smoothing.noise_reduction,
        },
        output,
    };
    toml::to_string(&cava).map_err(|e| InfraError::Cava(e.to_string()))
}

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
    /// Spawn `cava` with a config derived from `config`.
    ///
    /// # Errors
    /// Returns [`InfraError`] when the binary is missing, pipes fail, or
    /// the generated config cannot be delivered.
    #[instrument(skip(config), fields(bars = config.bars.amount))]
    pub fn spawn(config: &Config, bars: BarCount) -> Result<Self, InfraError> {
        let toml = cava_config_toml(config)?;
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

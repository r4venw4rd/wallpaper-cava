//! cava child-process adapter.
//!
//! The visualizer does not do DSP itself: it spawns `cava -p /dev/stdin`
//! with a generated `raw / 16bit` config and decodes the pipe with pure
//! domain logic.

pub mod config;
pub mod source;

pub use config::cava_config_toml;
pub use source::CavaSource;

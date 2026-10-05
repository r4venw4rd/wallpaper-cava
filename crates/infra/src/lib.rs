//! Infrastructure adapters: cava child process, GL helpers, Wayland shell.
//!
//! The only place where IO, FFI, and `unsafe` live. Every `unsafe` block
//! carries a `// SAFETY:` justification; every fallible operation returns
//! `Result` — no `unwrap`/`expect`/`panic`.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(missing_docs)]
#![warn(clippy::todo)]
#![warn(clippy::pedantic)]

pub mod cava;
pub mod error;
pub mod gl_util;
pub mod shell;

pub use cava::{cava_config_toml, CavaSource};
pub use error::InfraError;
pub use gl_util::{compile_shader, link_program};
pub use shell::WallpaperShell;

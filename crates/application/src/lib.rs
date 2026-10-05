//! Application services: wire ports to pure domain logic.
//!
//! Stateless orchestration only. No IO, no Wayland, no GL.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(missing_docs)]
#![warn(clippy::todo)]
#![warn(clippy::pedantic)]

pub mod services;

pub use services::frame::FrameService;

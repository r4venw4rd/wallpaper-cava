//! Wallpaper CAVA: spectrum visualizer on the Wayland wallpaper layer.
//!
//! Thin binary: CLI + config + logging setup, then dependency injection.
//! All fallible steps use `anyhow` with context; no `unwrap`/`expect`.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(missing_docs)]
#![warn(clippy::todo)]
#![warn(clippy::pedantic)]

extern crate khronos_egl as egl;

mod args;

use std::fs;

use anyhow::Context;
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::{globals::registry_queue_init, Connection};
use std::time::Duration;

use wallpaper_cava_config::Config;
use wallpaper_cava_render::{CavaSource, WallpaperShell};

use args::parse_args;

/// Vertex shader embedded at compile time.
const VERTEX_SHADER_SRC: &str = include_str!("vertex_shader.glsl");
/// Fragment shader embedded at compile time.
const FRAGMENT_SHADER_SRC: &str = include_str!("fragment_shader.glsl");

/// Target time between frames for a validated framerate value.
#[must_use]
fn frame_duration(framerate: u32) -> Duration {
    Duration::from_secs(1) / framerate.max(1)
}

/// Wire everything and run the Wayland event loop.
fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let raw: Vec<String> = std::env::args().collect();
    let args = parse_args(&raw);
    let config_str = fs::read_to_string(&args.config_path)
        .with_context(|| format!("reading config file {}", args.config_path))?;
    let config: Config = toml::from_str(&config_str)
        .with_context(|| format!("parsing config {}", args.config_path))?;

    // Fail fast on invalid ranges before touching Wayland/EGL.
    let framerate = config.framerate()?;
    let params = WallpaperShell::collect_params(
        &config,
        VERTEX_SHADER_SRC.to_string(),
        FRAGMENT_SHADER_SRC.to_string(),
    )?;
    if args.check_only {
        tracing::info!(
            config = %args.config_path,
            cava_bars = params.cava_bars.get(),
            framerate = framerate.get(),
            default_bars = params.default.bars.get(),
            outputs = params.outputs.len(),
            "config OK"
        );
        let mut names: Vec<&String> = params.outputs.keys().collect();
        names.sort();
        for name in names {
            let out = &params.outputs[name];
            tracing::info!(
                output = %name,
                bars = out.bars.get(),
                gradient_stops = out.gradient.len(),
                "output OK"
            );
        }
        return Ok(());
    }
    let frame_duration = frame_duration(framerate.get());

    let cava = CavaSource::spawn(&config, params.cava_bars)?;

    let conn = Connection::connect_to_env().context("connecting to Wayland")?;
    let (globals, event_queue) = registry_queue_init(&conn).context("initializing registry")?;
    let qh = event_queue.handle();

    let mut event_loop: EventLoop<WallpaperShell> =
        EventLoop::try_new().context("creating event loop")?;
    let loop_handle = event_loop.handle();
    WaylandSource::new(conn.clone(), event_queue)
        .insert(loop_handle)
        .map_err(|e| anyhow::anyhow!("inserting wayland source: {e:?}"))?;

    let mut shell = WallpaperShell::create(&conn, &globals, &qh, params, cava)?;

    event_loop
        .run(frame_duration, &mut shell, |_| {})
        .map_err(|e| anyhow::anyhow!("event loop failed: {e:?}"))?;
    Ok(())
}

/// Binary entry point: report errors with context, don't panic.
fn main() {
    if let Err(e) = run() {
        tracing::error!(error = ?e, "wallpaper-cava failed");
        std::process::exit(1);
    }
}

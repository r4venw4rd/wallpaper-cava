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

use std::fs;
use std::time::Duration;

use anyhow::Context;
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::{globals::registry_queue_init, Connection};
use wallpaper_cava_domain::Config;
use wallpaper_cava_infra::{CavaSource, WallpaperShell};

/// Vertex shader embedded at compile time.
const VERTEX_SHADER_SRC: &str = include_str!("vertex_shader.glsl");
/// Fragment shader embedded at compile time.
const FRAGMENT_SHADER_SRC: &str = include_str!("fragment_shader.glsl");

/// CLI usage, written to stdout. Stdout is correct here: this is the
/// program's primary output channel when misused.
#[allow(clippy::print_stdout)]
fn print_help() {
    println!("Command line options");
    println!("--config path");
}

/// Resolve which config file to load from `argv`.
///
/// Supports `--config <path>`, otherwise `$HOME/.config/wallpaper-cava/config.toml`
/// when present, falling back to `./config.toml`.
fn resolve_config_path(args: &[String]) -> String {
    if args.len() == 3 && args[1] == "--config" {
        return args[2].clone();
    }
    if args.len() != 1 {
        print_help();
        std::process::exit(0);
    }
    std::env::var("HOME")
        .map(|home| format!("{home}/.config/wallpaper-cava/config.toml"))
        .ok()
        .filter(|path| fs::metadata(path).is_ok())
        .unwrap_or_else(|| "config.toml".to_string())
}

/// Wire everything and run the Wayland event loop.
fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();
    let config_path = resolve_config_path(&args);
    let config_str = fs::read_to_string(&config_path)
        .with_context(|| format!("reading config file {config_path}"))?;
    let config: Config =
        toml::from_str(&config_str).with_context(|| format!("parsing config {config_path}"))?;

    // Fail fast on invalid ranges before touching Wayland/EGL.
    let bar_count = config.bar_count()?;
    let framerate = config.framerate()?;
    let frame_duration = Duration::from_secs(1) / framerate.get();

    let params = WallpaperShell::collect_params(
        &config,
        VERTEX_SHADER_SRC.to_string(),
        FRAGMENT_SHADER_SRC.to_string(),
    )?;
    let cava = CavaSource::spawn(&config, bar_count)?;

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

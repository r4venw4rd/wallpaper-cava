//! Wayland wallpaper shell: `wlr-layer-shell` surface + EGL/GL rendering.
//!
//! This is the platform adapter. Protocol errors are returned, never
//! panicked; frame-callback failures are logged and the loop continues.

mod bind;
mod display;
mod draw;
mod gl_objects;
mod handlers;
mod outputs;
mod params;
mod setup;
mod views;

pub use params::SelfParams;

use std::collections::HashMap;

use crate::outputs::ResolvedOutput;
use khronos_egl as egl;
use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::registry::RegistryState;
use smithay_client_toolkit::shell::wlr_layer::LayerShell;

use views::{OutputView, SetupSurface};
use wallpaper_cava_audio::CavaSource;

/// Placeholder size before the compositor sends the first configure.
pub(super) const PLACEHOLDER_SIZE: u32 = 256;

/// EGL framebuffer config: 8-bit RGBA.
pub(super) const EGL_ATTRIBUTES: [i32; 9] = [
    egl::RED_SIZE,
    8,
    egl::GREEN_SIZE,
    8,
    egl::BLUE_SIZE,
    8,
    egl::ALPHA_SIZE,
    8,
    egl::NONE,
];

/// EGL context: OpenGL 4.6 core.
pub(super) const CONTEXT_ATTRIBUTES: [i32; 7] = [
    egl::CONTEXT_MAJOR_VERSION,
    4,
    egl::CONTEXT_MINOR_VERSION,
    6,
    egl::CONTEXT_OPENGL_PROFILE_MASK,
    egl::CONTEXT_OPENGL_CORE_PROFILE_BIT,
    egl::NONE,
];

/// Wallpaper shell: one `wlr-layer-shell` surface per bound output, all
/// sharing a single EGL context and GL program. Created by
/// [`WallpaperShell::create`].
pub struct WallpaperShell {
    /// Registry state (SCTK requirement).
    pub(super) registry_state: RegistryState,
    /// Output state (SCTK requirement).
    pub(super) output_state: OutputState,
    /// Layer-shell protocol object.
    pub(super) layer_shell: LayerShell,
    /// Compositor handle (for creating surfaces as outputs appear).
    pub(super) compositor: CompositorState,
    /// Spectrum source (cava child).
    pub(super) cava: CavaSource,
    /// Consecutive cava read failures (log throttle; reset on success).
    pub(super) cava_failures: u32,
    /// Chosen EGL config (shared by all views).
    pub(super) egl_config: egl::Config,
    /// Shared EGL context.
    pub(super) egl_context: egl::Context,
    /// EGL display connection.
    pub(super) egl_display: egl::Display,
    /// Linked GL program (shared by all views).
    pub(super) shader_program: u32,
    /// Vertex array object (shared by all views).
    pub(super) vao: u32,
    /// Vertex buffer object (refilled per view, every frame).
    pub(super) vbo: u32,
    /// Gradient SSBO (refilled per view, every frame).
    pub(super) ssbo: u32,
    /// `WindowSize` uniform location.
    pub(super) window_size_location: i32,
    /// Global default settings (unnamed outputs).
    pub(super) default: ResolvedOutput,
    /// Per-output settings, keyed by output name.
    pub(super) outputs: HashMap<String, ResolvedOutput>,
    /// Preferred output name: when set, only that output gets a view
    /// (run one instance per monitor, each with its own config).
    /// When unset, every output gets a view.
    pub(super) preferred_output: Option<String>,
    /// Pre-bind placeholder surface. Holds the EGL context current for GL
    /// setup; retired when the first real view binds.
    pub(super) setup: Option<SetupSurface>,
    /// One live surface per bound output.
    pub(super) views: Vec<OutputView>,
}

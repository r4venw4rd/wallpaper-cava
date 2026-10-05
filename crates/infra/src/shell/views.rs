//! View structs plus setup-retire and output-unbind lifecycle.

use gl::types::GLsizei;
use smithay_client_toolkit::reexports::client::protocol::wl_output::WlOutput;
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use tracing::{error, info};
use wallpaper_cava_domain::{BarCount, GapRatio, Rgba};
use wayland_egl::WlEglSurface;

use super::WallpaperShell;

use egl::API as egl_api;
use khronos_egl as egl;

/// Pre-bind placeholder: an unplaced layer surface used only to hold the
/// EGL context current while GL objects are created.
pub(crate) struct SetupSurface {
    /// Placeholder Wayland surface.
    pub(crate) surface: WlSurface,
    /// Placeholder layer surface.
    pub(crate) layer_surface: LayerSurface,
    /// Placeholder EGL window binding.
    pub(crate) wl_egl_surface: WlEglSurface,
    /// Placeholder EGL surface.
    pub(crate) egl_surface: egl::Surface,
}

/// One bound output: its own layer surface + EGL surface, sharing the
/// shell's context, program, and buffers.
pub(crate) struct OutputView {
    /// Wayland output this view renders on.
    pub(crate) output: WlOutput,
    /// Output name at bind time (for logs).
    pub(crate) name: Option<String>,
    /// View Wayland surface.
    pub(crate) surface: WlSurface,
    /// View layer surface.
    pub(crate) layer_surface: LayerSurface,
    /// View EGL window binding (resized in place on configure).
    pub(crate) wl_egl_surface: WlEglSurface,
    /// View EGL surface.
    pub(crate) egl_surface: egl::Surface,
    /// Current width (px).
    pub(crate) width: u32,
    /// Current height (px).
    pub(crate) height: u32,
    /// True once the compositor sent the initial configure.
    pub(crate) configured: bool,
    /// Bar count for this view.
    pub(crate) bars: BarCount,
    /// Index count (`bars * 6`) for this view. Valid because per-bar
    /// indices form a prefix of the max-bars buffer uploaded at setup.
    pub(crate) index_count: GLsizei,
    /// Gap ratio for this view.
    pub(crate) gap: GapRatio,
    /// Clear color for this view.
    pub(crate) background: Rgba,
    /// Packed gradient SSBO bytes for this view.
    pub(crate) gradient_bytes: Vec<u8>,
}

impl WallpaperShell {
    /// Drop the setup placeholder (its EGL surface first, then the Wayland
    /// objects via `Drop`). Called once, when the first real view binds.
    ///
    /// Order matters: the layer role object must die BEFORE its
    /// `wl_surface` — the reverse is a protocol error and the compositor
    /// kills the connection (`invalid object`).
    pub(crate) fn retire_setup(&mut self) {
        if let Some(setup) = self.setup.take() {
            if let Err(e) = egl_api.make_current(self.egl_display, None, None, None) {
                error!(
                    error = format!("{e:?}"),
                    "make_current(None) during setup retire"
                );
            }
            if let Err(e) = egl_api.destroy_surface(self.egl_display, setup.egl_surface) {
                error!(error = format!("{e:?}"), "destroy setup EGL surface");
            }
            // Explicit drop order: EGL window binding, then layer role,
            // and only then the wl_surface itself.
            drop(setup.wl_egl_surface);
            drop(setup.layer_surface);
            setup.surface.destroy();
        }
    }

    /// Remove the view bound to `output`, if any, and destroy its EGL
    /// surface. Wayland objects die with the removed view — layer role
    /// before `wl_surface` (see [`Self::retire_setup`]).
    pub(crate) fn unbind_view(&mut self, output: &WlOutput) {
        if let Some(pos) = self.views.iter().position(|v| v.output == *output) {
            let view = self.views.remove(pos);
            info!(output = ?view.name, "output removed, unbinding view");
            if let Err(e) = egl_api.make_current(self.egl_display, None, None, None) {
                error!(error = format!("{e:?}"), "make_current(None) during unbind");
            }
            if let Err(e) = egl_api.destroy_surface(self.egl_display, view.egl_surface) {
                error!(error = format!("{e:?}"), "destroy view EGL surface");
            }
            drop(view.wl_egl_surface);
            drop(view.layer_surface);
            view.surface.destroy();
        }
    }
}

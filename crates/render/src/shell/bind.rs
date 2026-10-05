//! Output binding: fresh surface + layer surface + EGL surface per output.

use crate::color::gradient_ssbo_bytes;
use crate::outputs::ResolvedOutput;
use gl::types::GLsizei;
use smithay_client_toolkit::reexports::client::protocol::wl_output::WlOutput;
use smithay_client_toolkit::reexports::client::{Proxy, QueueHandle};
use smithay_client_toolkit::shell::wlr_layer::{Anchor, Layer};
use wayland_egl::WlEglSurface;

use super::{display, views::OutputView, WallpaperShell};
use crate::error::RenderError;

impl WallpaperShell {
    /// Create a fresh surface + layer surface + EGL surface on `output`.
    ///
    /// # Errors
    /// Returns [`RenderError`] when surface creation or EGL binding fails.
    #[allow(clippy::ptr_as_ptr)]
    pub(super) fn bind_view(
        &mut self,
        qh: &QueueHandle<Self>,
        output: &WlOutput,
        name: Option<String>,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        self.retire_setup();
        // Per-output settings, falling back to the global default.
        let resolved: &ResolvedOutput = self
            .outputs
            .get(name.as_deref().unwrap_or(""))
            .unwrap_or(&self.default);
        let (bars, gap, background) = (resolved.bars, resolved.gap, resolved.background);
        let gradient_bytes = gradient_ssbo_bytes(&resolved.gradient);
        let index_count = GLsizei::try_from(bars.get() * 6)
            .map_err(|_| RenderError::Gl("bar count does not fit GLsizei".to_string()))?;
        let surface = self.compositor.create_surface(qh);
        let empty_region = self.compositor.wl_compositor().create_region(qh, ());
        surface.set_input_region(Some(&empty_region));
        empty_region.destroy();
        let layer_surface = self.layer_shell.create_layer_surface(
            qh,
            surface.clone(),
            Layer::Bottom,
            Some("wallpaper-cava"),
            Some(output),
        );
        layer_surface.set_size(width, height);
        layer_surface.set_anchor(Anchor::TOP);
        surface.commit();
        let wl_egl_surface =
            WlEglSurface::new(surface.id(), width.cast_signed(), height.cast_signed())
                .map_err(|e| RenderError::Wayland(e.to_string()))?;
        let egl_surface = display::window_surface(
            self.egl_display,
            self.egl_config,
            wl_egl_surface.ptr() as khronos_egl::NativeWindowType,
        )?;
        self.views.push(OutputView {
            output: output.clone(),
            name,
            surface,
            layer_surface,
            wl_egl_surface,
            egl_surface,
            width,
            height,
            configured: false,
            bars,
            index_count,
            gap,
            background,
            gradient_bytes,
        });
        Ok(())
    }
}

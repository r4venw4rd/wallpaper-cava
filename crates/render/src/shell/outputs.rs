//! Output hotplug: bind on appear, resize in place on mode change.

use smithay_client_toolkit::reexports::client::protocol::wl_output::WlOutput;
use smithay_client_toolkit::reexports::client::QueueHandle;
use tracing::{debug, error, info};

use super::WallpaperShell;

impl WallpaperShell {
    /// Decide whether `output_name` should get a view.
    pub(super) fn wants_output(&self, output_name: Option<&str>) -> bool {
        match (&self.preferred_output, output_name) {
            (Some(want), Some(got)) => want == got,
            (None, _) => true,
            (Some(_), None) => false,
        }
    }

    /// Logical size of `output`, or `None` when the compositor reports
    /// nothing usable.
    pub(super) fn output_size(&self, output: &WlOutput) -> Option<(u32, u32)> {
        let info = self.output_state.info(output)?;
        let (w, h) = info.logical_size?;
        match (u32::try_from(w.max(1)), u32::try_from(h.max(1))) {
            (Ok(width), Ok(height)) => Some((width, height)),
            _ => None,
        }
    }

    /// A (possibly new) output announced itself: bind it unless filtered
    /// out or already bound. Never rebinds an existing view.
    pub(super) fn on_new_output(&mut self, qh: &QueueHandle<Self>, output: &WlOutput) {
        let name: Option<String> = self.output_state.info(output).and_then(|i| i.name);
        if !self.wants_output(name.as_deref()) {
            debug!(output = ?name, "ignoring non-preferred output");
            return;
        }
        if self.views.iter().any(|v| v.output == *output) {
            debug!(output = ?name, "output already bound, skipping");
            return;
        }
        let Some((width, height)) = self.output_size(output) else {
            error!(output = ?name, "output has no usable size, skipping bind");
            return;
        };
        if let Err(e) = self.bind_view(qh, output, name.clone(), width, height) {
            error!(error = %e, output = ?name, "failed to bind output");
        } else {
            info!(output = ?name, width, height, "bound to output");
        }
    }

    /// An output changed (mode/scale): resize its view in place when the
    /// size actually moved. No surface re-creation, no flicker.
    /// Unknown outputs fall through to [`Self::on_new_output`].
    pub(super) fn on_update_output(&mut self, qh: &QueueHandle<Self>, output: &WlOutput) {
        if !self.views.iter().any(|v| v.output == *output) {
            self.on_new_output(qh, output);
            return;
        }
        let Some((width, height)) = self.output_size(output) else {
            error!("updated output has no usable size, keeping current");
            return;
        };
        if let Some(view) = self.views.iter_mut().find(|v| v.output == *output) {
            if view.width != width || view.height != height {
                info!(output = ?view.name, width, height, "output resized");
                view.width = width;
                view.height = height;
                view.layer_surface.set_size(width, height);
                view.surface.commit();
                view.wl_egl_surface
                    .resize(width.cast_signed(), height.cast_signed(), 0, 0);
            }
        }
    }
}

//! SCTK handler wiring: outputs, compositor frames, layer configures.

use smithay_client_toolkit::compositor::CompositorHandler;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::client::protocol::wl_output::{Transform, WlOutput};
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::{delegate_noop, Connection, QueueHandle};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::registry_handlers;
use smithay_client_toolkit::shell::wlr_layer::{
    LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::{
    delegate_compositor, delegate_layer, delegate_output, delegate_registry,
};
use tracing::{error, info};

use super::WallpaperShell;

impl OutputHandler for WallpaperShell {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
        self.on_new_output(qh, &output);
    }

    fn update_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
        self.on_update_output(qh, &output);
    }

    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, output: WlOutput) {
        self.unbind_view(&output);
    }
}

delegate_compositor!(WallpaperShell);
delegate_noop!(WallpaperShell: ignore smithay_client_toolkit::reexports::client::protocol::wl_region::WlRegion);

delegate_output!(WallpaperShell);
delegate_registry!(WallpaperShell);
delegate_layer!(WallpaperShell);

impl ProvidesRegistryState for WallpaperShell {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![];
}

impl CompositorHandler for WallpaperShell {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_transform: Transform,
    ) {
    }

    fn frame(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &WlSurface,
        _time: u32,
    ) {
        // Ignore callbacks from unknown surfaces (e.g. the retired setup
        // surface): drawing there would be a protocol error and kill the
        // connection. Any known configured view triggers a full redraw.
        let known = self
            .views
            .iter()
            .any(|v| v.configured && v.surface == *surface);
        if known {
            if let Err(e) = self.draw(conn, qh) {
                error!(error = %e, "frame draw failed");
            }
        }
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &WlOutput,
    ) {
    }
}

impl LayerShellHandler for WallpaperShell {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {}

    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let Some(view) = self.views.iter_mut().find(|v| v.layer_surface == *layer) else {
            // Configure for an unknown (already unbound) surface.
            return;
        };
        // SCTK acks the configure before dispatching, so the surface may
        // now receive a buffer.
        view.configured = true;
        // A size of 0 means "client decides": keep the current size then.
        if configure.new_size.0 != 0 {
            view.width = configure.new_size.0;
        }
        if configure.new_size.1 != 0 {
            view.height = configure.new_size.1;
        }
        let (width, height, name) = (view.width, view.height, view.name.clone());
        info!(?name, width, height, "layer surface configured");
        view.wl_egl_surface
            .resize(width.cast_signed(), height.cast_signed(), 0, 0);
        view.surface.commit();
        if let Err(e) = self.draw(conn, qh) {
            error!(error = %e, "initial draw after configure failed");
        }
    }
}

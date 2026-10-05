//! Shell construction: placeholder surface, EGL, GL, then `Self`.

use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::reexports::client::globals::GlobalList;
use smithay_client_toolkit::reexports::client::{Connection, Proxy, QueueHandle};
use smithay_client_toolkit::registry::RegistryState;
use smithay_client_toolkit::shell::wlr_layer::{Anchor, Layer, LayerShell};
use wayland_egl::WlEglSurface;

use super::{display, gl_objects, WallpaperShell};
use super::{params::SelfParams, views::SetupSurface, PLACEHOLDER_SIZE};
use crate::cava::CavaSource;
use crate::error::InfraError;

use khronos_egl::API as egl_api;

impl WallpaperShell {
    /// Create the initial (unplaced) surface and all GL state.
    ///
    /// # Errors
    /// Returns [`InfraError`] when any Wayland/EGL/GL step fails.
    #[allow(clippy::ptr_as_ptr)]
    pub fn create(
        conn: &Connection,
        globals: &GlobalList,
        qh: &QueueHandle<Self>,
        params: SelfParams,
        cava: CavaSource,
    ) -> Result<Self, InfraError> {
        let compositor =
            CompositorState::bind(globals, qh).map_err(|e| InfraError::Wayland(e.to_string()))?;
        let surface = compositor.create_surface(qh);
        let empty_region = compositor.wl_compositor().create_region(qh, ());
        surface.set_input_region(Some(&empty_region));
        empty_region.destroy();

        let layer_shell =
            LayerShell::bind(globals, qh).map_err(|e| InfraError::Wayland(e.to_string()))?;
        let layer_surface = layer_shell.create_layer_surface(
            qh,
            surface.clone(),
            Layer::Bottom,
            Some("wallpaper-cava"),
            None,
        );
        layer_surface.set_size(PLACEHOLDER_SIZE, PLACEHOLDER_SIZE);
        layer_surface.set_anchor(Anchor::TOP);
        surface.commit();

        let (egl_display, egl_config, egl_context) = display::init_display(conn)?;
        let wl_egl_surface = WlEglSurface::new(
            surface.id(),
            PLACEHOLDER_SIZE.cast_signed(),
            PLACEHOLDER_SIZE.cast_signed(),
        )
        .map_err(|e| InfraError::Wayland(e.to_string()))?;
        let egl_surface = display::window_surface(
            egl_display,
            egl_config,
            wl_egl_surface.ptr() as khronos_egl::NativeWindowType,
        )?;
        egl_api
            .make_current(
                egl_display,
                Some(egl_surface),
                Some(egl_surface),
                Some(egl_context),
            )
            .map_err(|e| InfraError::Egl(format!("make_current: {e:?}")))?;

        let objects = gl_objects::init(
            &params.vertex_src,
            &params.fragment_src,
            &params.gradient_bytes,
            params.cava_bars,
        )?;

        Ok(Self {
            registry_state: RegistryState::new(globals),
            output_state: OutputState::new(globals, qh),
            layer_shell,
            compositor,
            cava,
            egl_config,
            egl_context,
            egl_display,
            shader_program: objects.program,
            vao: objects.vao,
            vbo: objects.vbo,
            ssbo: objects.ssbo,
            window_size_location: objects.window_size_location,
            default: params.default,
            outputs: params.outputs,
            preferred_output: params.preferred_output,
            setup: Some(SetupSurface {
                surface,
                layer_surface,
                wl_egl_surface,
                egl_surface,
            }),
            views: Vec::new(),
        })
    }
}

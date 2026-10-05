//! Wayland wallpaper shell: `wlr-layer-shell` surface + EGL/GL rendering.
//!
//! This is the platform adapter. Protocol errors are returned, never
//! panicked; frame-callback failures are logged and the loop continues.

use std::ffi::CStr;
use std::ptr;

use gl::types::GLsizei;
use khronos_egl as egl;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::client::globals::GlobalList;
use smithay_client_toolkit::reexports::client::protocol::wl_output::WlOutput;
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::reexports::client::{delegate_noop, Connection, Proxy, QueueHandle};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::registry_handlers;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::{
    delegate_compositor, delegate_layer, delegate_output, delegate_registry,
};
use tracing::{debug, error, info, instrument};
use wallpaper_cava_domain::{
    array_from_config_color, gradient_ssbo_bytes, indices_for_bars, vertices_for_levels,
    AudioSource, BarCount, Config, GapRatio, Rgba,
};
use wayland_egl::WlEglSurface;

use crate::cava::CavaSource;
use crate::error::InfraError;
use crate::gl_util::{compile_shader, link_program};

use egl::API as egl_api;

/// Placeholder size before the compositor sends the first configure.
const PLACEHOLDER_SIZE: u32 = 256;

/// EGL framebuffer config: 8-bit RGBA.
const EGL_ATTRIBUTES: [i32; 9] = [
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
const CONTEXT_ATTRIBUTES: [i32; 7] = [
    egl::CONTEXT_MAJOR_VERSION,
    4,
    egl::CONTEXT_MINOR_VERSION,
    6,
    egl::CONTEXT_OPENGL_PROFILE_MASK,
    egl::CONTEXT_OPENGL_CORE_PROFILE_BIT,
    egl::NONE,
];

/// Wallpaper surface: owns the layer surface, EGL state, GL objects, and
/// the audio source. Created by [`WallpaperShell::create`].
pub struct WallpaperShell {
    /// Registry state (SCTK requirement).
    registry_state: RegistryState,
    /// Output state (SCTK requirement).
    output_state: OutputState,
    /// Current surface width (px).
    width: u32,
    /// Current surface height (px).
    height: u32,
    /// Layer-shell protocol object.
    layer_shell: LayerShell,
    /// Active layer surface.
    layer_surface: LayerSurface,
    /// Active Wayland surface.
    surface: WlSurface,
    /// Spectrum source (cava child).
    cava: CavaSource,
    /// EGL window binding (resized in place on configure).
    wl_egl_surface: WlEglSurface,
    /// Current EGL surface.
    egl_surface: egl::Surface,
    /// Chosen EGL config (reused when rebinding outputs).
    egl_config: egl::Config,
    /// Shared EGL context.
    egl_context: egl::Context,
    /// EGL display connection.
    egl_display: egl::Display,
    /// Linked GL program.
    shader_program: u32,
    /// Vertex array object.
    vao: u32,
    /// Vertex buffer object (refilled every frame).
    vbo: u32,
    /// `WindowSize` uniform location.
    window_size_location: i32,
    /// Validated bar count.
    bar_count: BarCount,
    /// Validated gap ratio.
    gap: GapRatio,
    /// Clear color.
    background: Rgba,
    /// Preferred output name, if configured.
    preferred_output: Option<String>,
    /// Compositor handle (for re-creating surfaces on output changes).
    compositor: CompositorState,
    /// True once the current surface received its initial configure.
    configured: bool,
}

/// Parameters for [`WallpaperShell::create`] beyond connection handles.
pub struct ShellParams {
    /// Validated bar count.
    pub bar_count: BarCount,
    /// Validated gap ratio.
    pub gap: GapRatio,
    /// Clear color.
    pub background: Rgba,
    /// Preferred output name.
    pub preferred_output: Option<String>,
    /// Packed gradient SSBO bytes (domain layout).
    pub gradient_bytes: Vec<u8>,
    /// Vertex shader source.
    pub vertex_src: String,
    /// Fragment shader source.
    pub fragment_src: String,
}

/// Validated shell parameters (helper to keep [`WallpaperShell::create`]
/// honest about what it needs).
pub struct SelfParams {
    /// Validated bar count.
    pub bar_count: BarCount,
    /// Validated gap ratio.
    pub gap: GapRatio,
    /// Clear color.
    pub background: Rgba,
    /// Preferred output name.
    pub preferred_output: Option<String>,
    /// Packed gradient SSBO bytes.
    pub gradient_bytes: Vec<u8>,
    /// Vertex shader source.
    pub vertex_src: String,
    /// Fragment shader source.
    pub fragment_src: String,
}

impl WallpaperShell {
    /// Collect validated render parameters from a [`Config`].
    ///
    /// Gradient stops are sorted by key so `gradient_color_10` cannot
    /// silently land before `gradient_color_2` (`HashMap` order is random).
    ///
    /// # Errors
    /// Returns [`InfraError`] on malformed colors or out-of-range values.
    pub fn collect_params(
        config: &Config,
        vertex_src: String,
        fragment_src: String,
    ) -> Result<SelfParams, InfraError> {
        let bar_count = config.bar_count()?;
        let gap = config.gap_ratio()?;
        let background = array_from_config_color(&config.general.background_color)?;
        let mut stops: Vec<(&String, &wallpaper_cava_domain::ConfigColor)> =
            config.colors.iter().collect();
        stops.sort_by(|a, b| a.0.cmp(b.0));
        let mut rgba = Vec::with_capacity(stops.len());
        for (_, color) in stops {
            rgba.push(array_from_config_color(color)?);
        }
        if rgba.is_empty() {
            return Err(InfraError::Config(
                "at least one [colors] gradient stop is required".to_string(),
            ));
        }
        Ok(SelfParams {
            bar_count,
            gap,
            background,
            preferred_output: config.general.preferred_output.clone(),
            gradient_bytes: gradient_ssbo_bytes(&rgba),
            vertex_src,
            fragment_src,
        })
    }

    /// Create the initial (unplaced) surface and all GL state.
    ///
    /// # Errors
    /// Returns [`InfraError`] when any Wayland/EGL/GL step fails.
    #[allow(clippy::too_many_lines, clippy::ptr_as_ptr)]
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

        egl_api
            .bind_api(egl::OPENGL_API)
            .map_err(|e| InfraError::Egl(format!("bind_api: {e:?}")))?;
        // SAFETY: `conn.display().id().as_ptr()` is the live Wayland
        // display pointer for `conn`, which outlives this call. The
        // mutability cast matches the EGL C API (`void *display_id`).
        let egl_display = unsafe {
            egl_api
                .get_display(conn.display().id().as_ptr() as *mut std::ffi::c_void)
                .ok_or_else(|| InfraError::Egl("get_display returned null".to_string()))?
        };
        egl_api
            .initialize(egl_display)
            .map_err(|e| InfraError::Egl(format!("initialize: {e:?}")))?;

        let egl_config = egl_api
            .choose_first_config(egl_display, &EGL_ATTRIBUTES)
            .map_err(|e| InfraError::Egl(format!("choose_config: {e:?}")))?
            .ok_or_else(|| InfraError::Egl("no matching EGL config".to_string()))?;

        let egl_context = egl_api
            .create_context(egl_display, egl_config, None, &CONTEXT_ATTRIBUTES)
            .map_err(|e| InfraError::Egl(format!("create_context: {e:?}")))?;

        let wl_egl_surface = WlEglSurface::new(
            surface.id(),
            PLACEHOLDER_SIZE.cast_signed(),
            PLACEHOLDER_SIZE.cast_signed(),
        )
        .map_err(|e| InfraError::Wayland(e.to_string()))?;
        // SAFETY: `wl_egl_surface` is live and its native window pointer
        // stays valid until the surface is destroyed (we destroy the EGL
        // surface first on rebind). The `void *` cast matches the EGL C API.
        let egl_surface = unsafe {
            egl_api
                .create_window_surface(
                    egl_display,
                    egl_config,
                    wl_egl_surface.ptr() as egl::NativeWindowType,
                    None,
                )
                .map_err(|e| InfraError::Egl(format!("create_window_surface: {e:?}")))?
        };
        egl_api
            .make_current(
                egl_display,
                Some(egl_surface),
                Some(egl_surface),
                Some(egl_context),
            )
            .map_err(|e| InfraError::Egl(format!("make_current: {e:?}")))?;

        gl::load_with(|name| {
            egl_api
                .get_proc_address(name)
                .map_or(ptr::null(), |f| f as *const std::ffi::c_void)
        });
        // SAFETY: `GetString(VERSION)` returns either a valid string or
        // null; we check for null before constructing the `CStr`.
        let version = unsafe {
            let ptr = gl::GetString(gl::VERSION);
            if ptr.is_null() {
                String::from("unknown")
            } else {
                CStr::from_ptr(ptr.cast()).to_string_lossy().into_owned()
            }
        };
        info!(gl_version = %version, egl_version = ?egl_api.version(), "GL initialized");

        let vert = compile_shader(gl::VERTEX_SHADER, &params.vertex_src)?;
        let frag = compile_shader(gl::FRAGMENT_SHADER, &params.fragment_src)?;
        let program = link_program(vert, frag)?;
        // SAFETY: owned shader objects; deleted exactly once here.
        unsafe {
            gl::DeleteShader(vert);
            gl::DeleteShader(frag);
        }

        let indices = indices_for_bars(params.bar_count);
        let window_size_name = std::ffi::CString::new("WindowSize")
            .map_err(|_| InfraError::Gl("internal uniform name contains nul".to_string()))?;

        let (mut vao, mut vbo, mut ebo, mut ssbo) = (0, 0, 0, 0);
        let window_size_location;
        let stride = GLsizei::try_from(2 * std::mem::size_of::<f32>())
            .map_err(|_| InfraError::Gl("vertex stride does not fit GLsizei".to_string()))?;
        let index_bytes = (indices.len() * std::mem::size_of::<u16>()).cast_signed();
        let gradient_bytes = params.gradient_bytes.len().cast_signed();
        // SAFETY: all object names are freshly generated; buffer sizes
        // describe the exact slices passed; the SSBO layout matches the
        // shader's `std430` block (count + 12 padding bytes + vec4 array).
        unsafe {
            gl::GenVertexArrays(1, &raw mut vao);
            gl::BindVertexArray(vao);
            gl::GenBuffers(1, &raw mut vbo);
            gl::GenBuffers(1, &raw mut ebo);
            gl::GenBuffers(1, &raw mut ssbo);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, ebo);
            gl::BufferData(
                gl::ELEMENT_ARRAY_BUFFER,
                index_bytes,
                indices.as_ptr().cast(),
                gl::STATIC_DRAW,
            );
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, ssbo);
            gl::BufferData(
                gl::SHADER_STORAGE_BUFFER,
                gradient_bytes,
                params.gradient_bytes.as_ptr().cast(),
                gl::STATIC_DRAW,
            );
            gl::BindBufferBase(gl::SHADER_STORAGE_BUFFER, 0, ssbo);
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, 0);
            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, stride, ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::BindVertexArray(0);
            window_size_location = gl::GetUniformLocation(program, window_size_name.as_ptr());
        }
        if window_size_location < 0 {
            debug!("WindowSize uniform not found (location < 0)");
        }

        Ok(Self {
            registry_state: RegistryState::new(globals),
            output_state: OutputState::new(globals, qh),
            width: PLACEHOLDER_SIZE,
            height: PLACEHOLDER_SIZE,
            layer_shell,
            layer_surface,
            surface,
            cava,
            wl_egl_surface,
            egl_surface,
            egl_config,
            egl_context,
            egl_display,
            shader_program: program,
            vao,
            vbo,
            window_size_location,
            bar_count: params.bar_count,
            gap: params.gap,
            background: params.background,
            preferred_output: params.preferred_output,
            compositor,
            configured: false,
        })
    }

    /// Render one frame from the next audio levels.
    ///
    /// # Errors
    /// Returns [`InfraError`] when the audio source, GL upload, or buffer
    /// swap fails. Handlers log and continue instead of propagating.
    #[instrument(skip(self, _conn, qh), fields(w = self.width, h = self.height))]
    pub fn draw(&mut self, _conn: &Connection, qh: &QueueHandle<Self>) -> Result<(), InfraError> {
        let levels = self.cava.next_levels()?;
        let vertices = vertices_for_levels(&levels, self.bar_count, self.gap)?;
        let indices = indices_for_bars(self.bar_count);
        let vertex_bytes = (vertices.len() * std::mem::size_of::<f32>()).cast_signed();
        let index_count = GLsizei::try_from(indices.len())
            .map_err(|_| InfraError::Gl("index count does not fit GLsizei".to_string()))?;
        // Surface sizes are compositor-provided pixels, far below 2^24:
        // the f32 conversion is exact.
        #[allow(clippy::cast_precision_loss)]
        let (fw, fh) = (self.width as f32, self.height as f32);
        // SAFETY: `vao`/`vbo` are live objects owned by `self`; `vertices`
        // outlives the synchronous upload; counts describe the slices.
        unsafe {
            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                vertex_bytes,
                vertices.as_ptr().cast(),
                gl::DYNAMIC_DRAW,
            );
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::ClearColor(
                self.background.r(),
                self.background.g(),
                self.background.b(),
                self.background.a(),
            );
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::UseProgram(self.shader_program);
            gl::Uniform2f(self.window_size_location, fw, fh);
            gl::DrawElements(gl::TRIANGLES, index_count, gl::UNSIGNED_SHORT, ptr::null());
            gl::BindVertexArray(0);
        }
        // The frame callback must be requested BEFORE the commit
        // (eglSwapBuffers commits the wl_surface). A callback requested
        // after the commit stays pending until the next commit, which never
        // comes -> single frozen frame.
        self.surface.frame(qh, self.surface.clone());
        egl_api
            .swap_buffers(self.egl_display, self.egl_surface)
            .map_err(|e| InfraError::Egl(format!("swap_buffers: {e:?}")))?;
        Ok(())
    }

    /// Tear down the current EGL surface and re-create it on `output`.
    ///
    /// # Errors
    /// Returns [`InfraError`] when surface creation or EGL rebind fails.
    #[allow(clippy::ptr_as_ptr)]
    fn rebind_to_output(
        &mut self,
        qh: &QueueHandle<Self>,
        output: &WlOutput,
        width: u32,
        height: u32,
    ) -> Result<(), InfraError> {
        egl_api
            .make_current(self.egl_display, None, None, None)
            .map_err(|e| InfraError::Egl(format!("make_current(None): {e:?}")))?;
        egl_api
            .destroy_surface(self.egl_display, self.egl_surface)
            .map_err(|e| InfraError::Egl(format!("destroy_surface: {e:?}")))?;
        let old_surface = self.surface.clone();
        self.surface = self.compositor.create_surface(qh);
        let empty_region = self.compositor.wl_compositor().create_region(qh, ());
        self.surface.set_input_region(Some(&empty_region));
        empty_region.destroy();
        self.layer_surface = self.layer_shell.create_layer_surface(
            qh,
            self.surface.clone(),
            Layer::Bottom,
            Some("wallpaper-cava"),
            Some(output),
        );
        self.configured = false;
        self.width = width;
        self.height = height;
        self.layer_surface.set_size(self.width, self.height);
        self.layer_surface.set_anchor(Anchor::TOP);
        self.surface.commit();
        self.wl_egl_surface = WlEglSurface::new(
            self.surface.id(),
            self.width.cast_signed(),
            self.height.cast_signed(),
        )
        .map_err(|e| InfraError::Wayland(e.to_string()))?;
        // SAFETY: fresh `wl_egl_surface`, same contract as in `create`
        // (EGL C API takes the native window as `void *`).
        self.egl_surface = unsafe {
            egl_api
                .create_window_surface(
                    self.egl_display,
                    self.egl_config,
                    self.wl_egl_surface.ptr() as egl::NativeWindowType,
                    None,
                )
                .map_err(|e| InfraError::Egl(format!("create_window_surface: {e:?}")))?
        };
        egl_api
            .make_current(
                self.egl_display,
                Some(self.egl_surface),
                Some(self.egl_surface),
                Some(self.egl_context),
            )
            .map_err(|e| InfraError::Egl(format!("make_current: {e:?}")))?;
        old_surface.destroy();
        Ok(())
    }

    /// Decide whether `output_name` should trigger a (re)bind.
    fn wants_output(&self, output_name: Option<&str>) -> bool {
        match (&self.preferred_output, output_name) {
            (Some(want), Some(got)) => want == got,
            (None, _) => true,
            (Some(_), None) => false,
        }
    }

    /// Shared body for `new_output` / `update_output`.
    fn handle_output(&mut self, qh: &QueueHandle<Self>, output: &WlOutput) {
        let name: Option<String> = self.output_state.info(output).and_then(|i| i.name);
        if !self.wants_output(name.as_deref()) {
            debug!(output = ?name, "ignoring non-preferred output");
            return;
        }
        let Some(info) = self.output_state.info(output) else {
            error!("output info unavailable, skipping bind");
            return;
        };
        let Some((w, h)) = info.logical_size else {
            error!("output has no logical size, skipping bind");
            return;
        };
        let Ok(width) = u32::try_from(w.max(1)) else {
            error!(w, h, "output width does not fit u32, skipping bind");
            return;
        };
        let Ok(height) = u32::try_from(h.max(1)) else {
            error!(w, h, "output height does not fit u32, skipping bind");
            return;
        };
        if let Err(e) = self.rebind_to_output(qh, output, width, height) {
            error!(error = %e, "failed to bind output");
        } else {
            info!(output = ?name, width, height, "bound to output");
        }
    }
}

// --- SCTK handler wiring -------------------------------------------------

impl OutputHandler for WallpaperShell {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
        self.handle_output(qh, &output);
    }

    fn update_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
        self.handle_output(qh, &output);
    }

    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
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
        _surface: &smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface,
        _new_transform: smithay_client_toolkit::reexports::client::protocol::wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface,
        _time: u32,
    ) {
        // Ignore callbacks left over from a replaced surface: drawing then
        // would attach a buffer to a not-yet-configured layer surface,
        // which is a protocol error and kills the connection.
        if *surface == self.surface && self.configured {
            if let Err(e) = self.draw(conn, qh) {
                error!(error = %e, "frame draw failed");
            }
        }
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface,
        _output: &WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface,
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
        if layer != &self.layer_surface {
            return;
        }
        // SCTK acks the configure before dispatching, so the surface may
        // now receive a buffer.
        self.configured = true;
        // A size of 0 means "client decides": keep the current size then.
        let width = if configure.new_size.0 != 0 {
            configure.new_size.0
        } else {
            self.width
        };
        let height = if configure.new_size.1 != 0 {
            configure.new_size.1
        } else {
            self.height
        };
        info!(width, height, "layer surface configured");
        self.width = width;
        self.height = height;
        self.wl_egl_surface
            .resize(self.width.cast_signed(), self.height.cast_signed(), 0, 0);
        // SAFETY: GL context is current on this thread; dimensions are the
        // live surface size.
        unsafe {
            gl::Viewport(0, 0, self.width.cast_signed(), self.height.cast_signed());
        }
        if let Err(e) = self.draw(conn, qh) {
            error!(error = %e, "initial draw after configure failed");
        }
    }
}

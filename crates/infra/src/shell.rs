//! Wayland wallpaper shell: `wlr-layer-shell` surface + EGL/GL rendering.
//!
//! This is the platform adapter. Protocol errors are returned, never
//! panicked; frame-callback failures are logged and the loop continues.

use std::collections::HashMap;
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
    gradient_ssbo_bytes, indices_for_bars, resample_levels, resolve_output, vertices_for_levels,
    AudioSource, BarCount, Config, GapRatio, ResolvedOutput, Rgba,
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

/// Wallpaper shell: one `wlr-layer-shell` surface per bound output, all
/// sharing a single EGL context and GL program. Created by
/// [`WallpaperShell::create`].
pub struct WallpaperShell {
    /// Registry state (SCTK requirement).
    registry_state: RegistryState,
    /// Output state (SCTK requirement).
    output_state: OutputState,
    /// Layer-shell protocol object.
    layer_shell: LayerShell,
    /// Compositor handle (for creating surfaces as outputs appear).
    compositor: CompositorState,
    /// Spectrum source (cava child).
    cava: CavaSource,
    /// Chosen EGL config (shared by all views).
    egl_config: egl::Config,
    /// Shared EGL context.
    egl_context: egl::Context,
    /// EGL display connection.
    egl_display: egl::Display,
    /// Linked GL program (shared by all views).
    shader_program: u32,
    /// Vertex array object (shared by all views).
    vao: u32,
    /// Vertex buffer object (refilled per view, every frame).
    vbo: u32,
    /// Gradient SSBO (refilled per view, every frame).
    ssbo: u32,
    /// `WindowSize` uniform location.
    window_size_location: i32,
    /// Global default settings (unnamed outputs).
    default: ResolvedOutput,
    /// Per-output settings, keyed by output name.
    outputs: HashMap<String, ResolvedOutput>,
    /// Preferred output name: when set, only that output gets a view
    /// (run one instance per monitor, each with its own config).
    /// When unset, every output gets a view.
    preferred_output: Option<String>,
    /// Pre-bind placeholder surface. Holds the EGL context current for GL
    /// setup; retired when the first real view binds.
    setup: Option<SetupSurface>,
    /// One live surface per bound output.
    views: Vec<OutputView>,
}

/// Pre-bind placeholder: an unplaced layer surface used only to hold the
/// EGL context current while GL objects are created.
struct SetupSurface {
    /// Placeholder Wayland surface.
    surface: WlSurface,
    /// Placeholder layer surface.
    layer_surface: LayerSurface,
    /// Placeholder EGL window binding.
    wl_egl_surface: WlEglSurface,
    /// Placeholder EGL surface.
    egl_surface: egl::Surface,
}

/// One bound output: its own layer surface + EGL surface, sharing the
/// shell's context, program, and buffers.
struct OutputView {
    /// Wayland output this view renders on.
    output: WlOutput,
    /// Output name at bind time (for logs).
    name: Option<String>,
    /// View Wayland surface.
    surface: WlSurface,
    /// View layer surface.
    layer_surface: LayerSurface,
    /// View EGL window binding (resized in place on configure).
    wl_egl_surface: WlEglSurface,
    /// View EGL surface.
    egl_surface: egl::Surface,
    /// Current width (px).
    width: u32,
    /// Current height (px).
    height: u32,
    /// True once the compositor sent the initial configure.
    configured: bool,
    /// Bar count for this view.
    bars: BarCount,
    /// Index count (`bars * 6`) for this view. Valid because per-bar
    /// indices form a prefix of the max-bars buffer uploaded at setup.
    index_count: GLsizei,
    /// Gap ratio for this view.
    gap: GapRatio,
    /// Clear color for this view.
    background: Rgba,
    /// Packed gradient SSBO bytes for this view.
    gradient_bytes: Vec<u8>,
}

/// Validated shell parameters (helper to keep [`WallpaperShell::create`]
/// honest about what it needs).
pub struct SelfParams {
    /// Bar count cava must run at (max over all views).
    pub cava_bars: BarCount,
    /// Global default settings.
    pub default: ResolvedOutput,
    /// Per-output settings, keyed by output name.
    pub outputs: HashMap<String, ResolvedOutput>,
    /// Preferred output name (single-view filter, kept for compatibility).
    pub preferred_output: Option<String>,
    /// Packed gradient SSBO bytes for the default (setup upload).
    pub gradient_bytes: Vec<u8>,
    /// Vertex shader source.
    pub vertex_src: String,
    /// Fragment shader source.
    pub fragment_src: String,
}

impl WallpaperShell {
    /// Collect validated render parameters from a [`Config`].
    ///
    /// Resolves the global default plus every `[outputs."NAME"]` section
    /// via pure domain logic.
    ///
    /// # Errors
    /// Returns [`InfraError`] on malformed colors or out-of-range values.
    pub fn collect_params(
        config: &Config,
        vertex_src: String,
        fragment_src: String,
    ) -> Result<SelfParams, InfraError> {
        let default = resolve_output(None, config)?;
        let mut outputs = HashMap::with_capacity(config.outputs.len());
        for name in config.outputs.keys() {
            outputs.insert(name.clone(), resolve_output(Some(name), config)?);
        }
        let cava_bars = wallpaper_cava_domain::cava_bars(config)?;
        let gradient_bytes = gradient_ssbo_bytes(&default.gradient);
        Ok(SelfParams {
            cava_bars,
            default,
            outputs,
            preferred_output: config.general.preferred_output.clone(),
            gradient_bytes,
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

        let indices = indices_for_bars(params.cava_bars);
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
            layer_shell,
            compositor,
            cava,
            egl_config,
            egl_context,
            egl_display,
            shader_program: program,
            vao,
            vbo,
            ssbo,
            window_size_location,
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

    /// Render one frame on every configured view.
    ///
    /// Levels are fetched once at cava resolution, then resampled per
    /// view; each view uploads its own vertices + gradient and draws.
    ///
    /// # Errors
    /// Returns [`InfraError`] when the audio source, GL upload, or buffer
    /// swap fails. Handlers log and continue instead of propagating.
    #[instrument(skip(self, _conn, qh), fields(views = self.views.len()))]
    pub fn draw(&mut self, _conn: &Connection, qh: &QueueHandle<Self>) -> Result<(), InfraError> {
        let mut active: Vec<usize> = self
            .views
            .iter()
            .enumerate()
            .filter(|(_, v)| v.configured)
            .map(|(i, _)| i)
            .collect();
        if active.is_empty() {
            return Ok(());
        }
        // Deterministic order: stable frame pacing across outputs.
        active.sort_unstable();
        let full_levels = self.cava.next_levels()?;
        for i in active {
            self.render_view(i, &full_levels, qh)?;
        }
        Ok(())
    }

    /// Make `views[i]` current on the shared context.
    fn make_view_current(&self, i: usize) -> Result<(), InfraError> {
        egl_api
            .make_current(
                self.egl_display,
                Some(self.views[i].egl_surface),
                Some(self.views[i].egl_surface),
                Some(self.egl_context),
            )
            .map_err(|e| InfraError::Egl(format!("make_current: {e:?}")))
    }

    /// Draw the already-uploaded frame on one view and swap its buffers.
    #[allow(clippy::cast_precision_loss)] // compositor pixels, far below 2^24: exact.
    /// Render the full-resolution levels on one view: resample to the
    /// view's bar count, upload vertices + gradient, draw, swap.
    #[allow(clippy::cast_precision_loss)] // compositor pixels, far below 2^24: exact.
    fn render_view(
        &mut self,
        i: usize,
        full_levels: &[f32],
        qh: &QueueHandle<Self>,
    ) -> Result<(), InfraError> {
        let view = &self.views[i];
        let (bars, gap, background) = (view.bars, view.gap, view.background);
        let (w, h) = (view.width, view.height);
        let (index_count, gradient_bytes) = (view.index_count, view.gradient_bytes.clone());
        let levels = resample_levels(full_levels, bars.get() as usize);
        let vertices = vertices_for_levels(&levels, bars, gap)?;
        let vertex_bytes = (vertices.len() * std::mem::size_of::<f32>()).cast_signed();
        let gradient_len = gradient_bytes.len().cast_signed();
        self.make_view_current(i)?;
        // SAFETY: context is current for this view; all sizes describe
        // the slices passed; the SSBO layout matches the shader block.
        unsafe {
            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                vertex_bytes,
                vertices.as_ptr().cast(),
                gl::DYNAMIC_DRAW,
            );
            gl::BindBuffer(gl::SHADER_STORAGE_BUFFER, self.ssbo);
            gl::BufferData(
                gl::SHADER_STORAGE_BUFFER,
                gradient_len,
                gradient_bytes.as_ptr().cast(),
                gl::DYNAMIC_DRAW,
            );
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::Viewport(0, 0, w.cast_signed(), h.cast_signed());
            gl::ClearColor(
                background.r(),
                background.g(),
                background.b(),
                background.a(),
            );
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::UseProgram(self.shader_program);
            gl::Uniform2f(self.window_size_location, w as f32, h as f32);
            gl::DrawElements(gl::TRIANGLES, index_count, gl::UNSIGNED_SHORT, ptr::null());
            gl::BindVertexArray(0);
        }
        // The frame callback must be requested BEFORE the commit
        // (eglSwapBuffers commits the wl_surface). A callback requested
        // after the commit stays pending until the next commit, which never
        // comes -> single frozen frame.
        self.views[i]
            .surface
            .frame(qh, self.views[i].surface.clone());
        egl_api
            .swap_buffers(self.egl_display, self.views[i].egl_surface)
            .map_err(|e| InfraError::Egl(format!("swap_buffers: {e:?}")))?;
        Ok(())
    }

    /// Drop the setup placeholder (its EGL surface first, then the Wayland
    /// objects via `Drop`). Called once, when the first real view binds.
    ///
    /// Order matters: the layer role object must die BEFORE its
    /// `wl_surface` — the reverse is a protocol error and the compositor
    /// kills the connection (`invalid object`).
    fn retire_setup(&mut self) {
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

    /// Create a fresh surface + layer surface + EGL surface on `output`.
    ///
    /// # Errors
    /// Returns [`InfraError`] when surface creation or EGL binding fails.
    #[allow(clippy::ptr_as_ptr)]
    fn bind_view(
        &mut self,
        qh: &QueueHandle<Self>,
        output: &WlOutput,
        name: Option<String>,
        width: u32,
        height: u32,
    ) -> Result<(), InfraError> {
        self.retire_setup();
        // Per-output settings, falling back to the global default.
        let resolved: &ResolvedOutput = self
            .outputs
            .get(name.as_deref().unwrap_or(""))
            .unwrap_or(&self.default);
        let (bars, gap, background) = (resolved.bars, resolved.gap, resolved.background);
        let gradient_bytes = gradient_ssbo_bytes(&resolved.gradient);
        let index_count = GLsizei::try_from(bars.get() * 6)
            .map_err(|_| InfraError::Gl("bar count does not fit GLsizei".to_string()))?;
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
                .map_err(|e| InfraError::Wayland(e.to_string()))?;
        // SAFETY: fresh `wl_egl_surface`; the EGL surface is destroyed
        // before it on unbind. The `void *` cast matches the EGL C API.
        let egl_surface = unsafe {
            egl_api
                .create_window_surface(
                    self.egl_display,
                    self.egl_config,
                    wl_egl_surface.ptr() as egl::NativeWindowType,
                    None,
                )
                .map_err(|e| InfraError::Egl(format!("create_window_surface: {e:?}")))?
        };
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

    /// Remove the view bound to `output`, if any, and destroy its EGL
    /// surface. Wayland objects die with the removed view — layer role
    /// before `wl_surface` (see [`Self::retire_setup`]).
    fn unbind_view(&mut self, output: &WlOutput) {
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

    /// Decide whether `output_name` should get a view.
    fn wants_output(&self, output_name: Option<&str>) -> bool {
        match (&self.preferred_output, output_name) {
            (Some(want), Some(got)) => want == got,
            (None, _) => true,
            (Some(_), None) => false,
        }
    }

    /// Logical size of `output`, or `None` when the compositor reports
    /// nothing usable.
    fn output_size(&self, output: &WlOutput) -> Option<(u32, u32)> {
        let info = self.output_state.info(output)?;
        let (w, h) = info.logical_size?;
        match (u32::try_from(w.max(1)), u32::try_from(h.max(1))) {
            (Ok(width), Ok(height)) => Some((width, height)),
            _ => None,
        }
    }

    /// A (possibly new) output announced itself: bind it unless filtered
    /// out or already bound. Never rebinds an existing view.
    fn on_new_output(&mut self, qh: &QueueHandle<Self>, output: &WlOutput) {
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
    fn on_update_output(&mut self, qh: &QueueHandle<Self>, output: &WlOutput) {
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

// --- SCTK handler wiring -------------------------------------------------

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

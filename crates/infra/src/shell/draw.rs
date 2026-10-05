//! Frame rendering: fetch levels once, resample + draw per view.

use std::ptr;

use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
use tracing::{debug, error, instrument};
use wallpaper_cava_domain::{resample_levels, vertices_for_levels, AudioSource};

use super::WallpaperShell;
use crate::error::InfraError;

use egl::API as egl_api;
use khronos_egl as egl;

impl WallpaperShell {
    /// Render one frame on every configured view.
    ///
    /// Levels are fetched once at cava resolution, then resampled per
    /// view; each view uploads its own vertices + gradient and draws.
    ///
    /// A dead cava holds the last frame instead of erroring every
    /// callback: audio failures are throttled-logged here (first + every
    /// 600th) and reported as `Ok`, so a 60fps loop cannot spam the log.
    /// GL/swap failures still propagate as [`InfraError`].
    ///
    /// # Errors
    /// Returns [`InfraError`] when the GL upload or buffer swap fails.
    /// Handlers log and continue instead of propagating.
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
        let full_levels = match self.cava.next_levels() {
            Ok(levels) => {
                self.cava_failures = 0;
                levels
            }
            Err(e) => {
                self.cava_failures = self.cava_failures.saturating_add(1);
                // ponytail: throttle — first + every 600th (≈10s at 60fps),
                // debug in between. The views keep their last frame.
                if self.cava_failures == 1 || self.cava_failures.is_multiple_of(600) {
                    error!(error = %e, consecutive_failures = self.cava_failures, "cava read failed, holding last frame");
                } else {
                    debug!(error = %e, consecutive_failures = self.cava_failures, "cava read failed");
                }
                return Ok(());
            }
        };
        for i in active {
            self.render_view(i, &full_levels, qh)?;
        }
        Ok(())
    }

    /// Make `views[i]` current on the shared context.
    pub(super) fn make_view_current(&self, i: usize) -> Result<(), InfraError> {
        egl_api
            .make_current(
                self.egl_display,
                Some(self.views[i].egl_surface),
                Some(self.views[i].egl_surface),
                Some(self.egl_context),
            )
            .map_err(|e| InfraError::Egl(format!("make_current: {e:?}")))
    }

    /// Resample full-resolution levels to the view's bar count, upload
    /// vertices + gradient, draw, and swap buffers.
    #[allow(clippy::cast_precision_loss)] // compositor pixels, far below 2^24: exact.
    pub(super) fn render_view(
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
}

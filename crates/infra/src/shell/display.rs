//! EGL display, config, context, and window-surface constructors.

use khronos_egl as egl;
use smithay_client_toolkit::reexports::client::{Connection, Proxy};

use super::{CONTEXT_ATTRIBUTES, EGL_ATTRIBUTES};
use crate::error::InfraError;

use egl::API as egl_api;

/// Open an EGL display on the Wayland connection and create the shared
/// config + OpenGL 4.6 core context.
///
/// # Errors
/// Returns [`InfraError::Egl`] when any EGL step fails.
#[allow(clippy::ptr_as_ptr)] // EGL C API takes `void *display_id` for a live pointer.
pub(super) fn init_display(
    conn: &Connection,
) -> Result<(egl::Display, egl::Config, egl::Context), InfraError> {
    egl_api
        .bind_api(egl::OPENGL_API)
        .map_err(|e| InfraError::Egl(format!("bind_api: {e:?}")))?;
    // SAFETY: `conn.display().id().as_ptr()` is the live Wayland
    // display pointer for `conn`, which outlives this call. The
    // mutability cast matches the EGL C API (`void *display_id`).
    let display = unsafe {
        egl_api
            .get_display(conn.display().id().as_ptr() as *mut std::ffi::c_void)
            .ok_or_else(|| InfraError::Egl("get_display returned null".to_string()))?
    };
    egl_api
        .initialize(display)
        .map_err(|e| InfraError::Egl(format!("initialize: {e:?}")))?;
    let config = egl_api
        .choose_first_config(display, &EGL_ATTRIBUTES)
        .map_err(|e| InfraError::Egl(format!("choose_config: {e:?}")))?
        .ok_or_else(|| InfraError::Egl("no matching EGL config".to_string()))?;
    let context = egl_api
        .create_context(display, config, None, &CONTEXT_ATTRIBUTES)
        .map_err(|e| InfraError::Egl(format!("create_context: {e:?}")))?;
    Ok((display, config, context))
}

/// Create a window surface for a live `WlEglSurface` native window.
///
/// # Errors
/// Returns [`InfraError::Egl`] when surface creation fails.
pub(super) fn window_surface(
    display: egl::Display,
    config: egl::Config,
    native: egl::NativeWindowType,
) -> Result<egl::Surface, InfraError> {
    // SAFETY: `native` comes from a live `WlEglSurface` whose EGL surface
    // is always destroyed before it (see retire/unbind). The `void *`
    // cast matches the EGL C API.
    unsafe {
        egl_api
            .create_window_surface(display, config, native, None)
            .map_err(|e| InfraError::Egl(format!("create_window_surface: {e:?}")))
    }
}

//! Shared GL objects: linked program, VAO/VBO/EBO, gradient SSBO.

use std::ffi::CStr;
use std::ptr;

use gl::types::GLsizei;
use tracing::{debug, info};
use wallpaper_cava_domain::{indices_for_bars, BarCount};

use crate::error::InfraError;
use crate::gl_util::{compile_shader, link_program};

use egl::API as egl_api;
use khronos_egl as egl;

/// GL objects shared by all views (one context, one program).
pub(super) struct GlObjects {
    /// Linked shader program.
    pub(super) program: u32,
    /// Vertex array object.
    pub(super) vao: u32,
    /// Vertex buffer object.
    pub(super) vbo: u32,
    /// Gradient SSBO.
    pub(super) ssbo: u32,
    /// `WindowSize` uniform location.
    pub(super) window_size_location: i32,
}

/// Load GL, compile + link shaders, upload the max-bars index buffer and
/// the default gradient. The context must already be current.
///
/// # Errors
/// Returns [`InfraError::Gl`] when shader compile/link or GL setup fails.
pub(super) fn init(
    vertex_src: &str,
    fragment_src: &str,
    gradient_bytes: &[u8],
    max_bars: BarCount,
) -> Result<GlObjects, InfraError> {
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

    let vert = compile_shader(gl::VERTEX_SHADER, vertex_src)?;
    let frag = compile_shader(gl::FRAGMENT_SHADER, fragment_src)?;
    let program = link_program(vert, frag)?;
    // SAFETY: owned shader objects; deleted exactly once here.
    unsafe {
        gl::DeleteShader(vert);
        gl::DeleteShader(frag);
    }

    let indices = indices_for_bars(max_bars);
    let window_size_name = std::ffi::CString::new("WindowSize")
        .map_err(|_| InfraError::Gl("internal uniform name contains nul".to_string()))?;
    let (mut vao, mut vbo, mut ebo, mut ssbo) = (0, 0, 0, 0);
    let window_size_location;
    let stride = GLsizei::try_from(2 * std::mem::size_of::<f32>())
        .map_err(|_| InfraError::Gl("vertex stride does not fit GLsizei".to_string()))?;
    let index_bytes = (indices.len() * std::mem::size_of::<u16>()).cast_signed();
    let gradient_len = gradient_bytes.len().cast_signed();
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
            gradient_len,
            gradient_bytes.as_ptr().cast(),
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
    Ok(GlObjects {
        program,
        vao,
        vbo,
        ssbo,
        window_size_location,
    })
}

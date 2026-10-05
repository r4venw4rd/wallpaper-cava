//! Minimal OpenGL program helpers with checked errors.
//!
//! The original code compiled shaders without checking status; link failures
//! panicked with an unbounded log read. Here every step is checked and the
//! info log is bounded.

use std::ffi::CString;

use crate::error::InfraError;

/// Upper bound for shader/program info logs (bytes).
const MAX_INFO_LOG: i32 = 4096;

/// Read a bounded info log for `object` via `get_len` / `get_log`.
fn bounded_info_log(
    mut get_len: impl FnMut(&mut i32),
    mut get_log: impl FnMut(i32, &mut i32, *mut std::ffi::c_void),
) -> String {
    let mut len: i32 = 0;
    get_len(&mut len);
    let clamped = len.clamp(0, MAX_INFO_LOG);
    if clamped == 0 {
        return String::new();
    }
    let Ok(clamped_usize) = usize::try_from(clamped) else {
        return String::new();
    };
    let mut buf: Vec<u8> = vec![0; clamped_usize];
    let mut written: i32 = 0;
    get_log(clamped, &mut written, buf.as_mut_ptr().cast());
    let Ok(valid) = usize::try_from(written.clamp(0, clamped)) else {
        return String::new();
    };
    buf.truncate(valid);
    String::from_utf8_lossy(&buf).into_owned()
}

/// Compile one shader stage.
///
/// # Errors
/// Returns [`InfraError::Gl`] on nul bytes, creation failure, or compile
/// errors (with the driver log attached).
pub fn compile_shader(shader_type: u32, source: &str) -> Result<u32, InfraError> {
    let c_source = CString::new(source)
        .map_err(|_| InfraError::Gl("shader source contains nul".to_string()))?;
    // SAFETY: `CreateShader` with a valid enum returns either a shader
    // object or 0; no preconditions on caller state.
    let shader = unsafe { gl::CreateShader(shader_type) };
    if shader == 0 {
        return Err(InfraError::Gl("glCreateShader returned 0".to_string()));
    }
    // SAFETY: `c_source` is a valid nul-terminated string that outlives
    // this call; count/length args describe exactly one string.
    unsafe {
        gl::ShaderSource(shader, 1, &c_source.as_ptr(), std::ptr::null());
        gl::CompileShader(shader);
    }
    let mut status = gl::types::GLint::from(gl::FALSE);
    // SAFETY: `shader` is a live shader object; `status` is a valid
    // out-pointer for one `GLint`.
    unsafe {
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &raw mut status);
    }
    if status == gl::types::GLint::from(gl::TRUE) {
        return Ok(shader);
    }
    let log = bounded_info_log(
        // SAFETY: `shader` is live; `len` is a valid out-pointer.
        |len| unsafe { gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, len) },
        // SAFETY: `buf` points to `buf_len` writable bytes; called once.
        |buf_len, written, buf| unsafe {
            gl::GetShaderInfoLog(shader, buf_len, written, buf.cast::<gl::types::GLchar>());
        },
    );
    // SAFETY: `shader` was created above and is not yet attached anywhere.
    unsafe {
        gl::DeleteShader(shader);
    }
    Err(InfraError::Gl(format!("shader compile failed: {log}")))
}

/// Link a vertex + fragment shader into a program.
///
/// # Errors
/// Returns [`InfraError::Gl`] on link failure (log attached). Shaders are
/// detached (not deleted — ownership stays with the caller) either way.
pub fn link_program(vert: u32, frag: u32) -> Result<u32, InfraError> {
    // SAFETY: `CreateProgram` has no preconditions; returns 0 on failure.
    let program = unsafe { gl::CreateProgram() };
    if program == 0 {
        return Err(InfraError::Gl("glCreateProgram returned 0".to_string()));
    }
    // SAFETY: `program` is live; `vert`/`frag` are compiled shader objects
    // owned by the caller and outlive this call.
    unsafe {
        gl::AttachShader(program, vert);
        gl::AttachShader(program, frag);
        gl::LinkProgram(program);
    }
    let mut status = gl::types::GLint::from(gl::FALSE);
    // SAFETY: `program` is live; `status` is a valid out-pointer.
    unsafe {
        gl::GetProgramiv(program, gl::LINK_STATUS, &raw mut status);
    }
    if status == gl::types::GLint::from(gl::TRUE) {
        // SAFETY: attached shaders can be detached exactly once each.
        unsafe {
            gl::DetachShader(program, vert);
            gl::DetachShader(program, frag);
        }
        return Ok(program);
    }
    let log = bounded_info_log(
        // SAFETY: `program` is live; `len` is a valid out-pointer.
        |len| unsafe { gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, len) },
        // SAFETY: `buf` points to `buf_len` writable bytes; called once.
        |buf_len, written, buf| unsafe {
            gl::GetProgramInfoLog(program, buf_len, written, buf.cast::<gl::types::GLchar>());
        },
    );
    // SAFETY: `program` was created above; detach before delete is valid.
    unsafe {
        gl::DetachShader(program, vert);
        gl::DetachShader(program, frag);
        gl::DeleteProgram(program);
    }
    Err(InfraError::Gl(format!("program link failed: {log}")))
}

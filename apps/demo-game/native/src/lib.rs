//! Application-owned native factory for the complete Endless Arena Runtime.
//!
//! The shared `kitu-unity-ffi` crate owns ABI lifetimes, ordered input admission,
//! output buffering and diagnostics. This crate selects the same application
//! factory and validated content used by the server, then exports thin C wrappers.
//! An optional loopback bridge exposes that same host to CLI/Admin. Only the
//! caller advances the clock; the bridge never creates a second game Runtime.
//!
//! See `doc/specs/arena-native-abi.md` and the shared C header for caller rules.

use kitu_demo_game::{arena, build_arena_runtime, build_demo_runtime};
use kitu_unity_ffi::application::{self as ffi, ApplicationDriver, ApplicationHandle};
use serde::Deserialize;
use std::path::PathBuf;

mod embedded;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NativeConfig {
    #[serde(default = "contract_version")]
    contract_version: u32,
    #[serde(default)]
    content: Option<arena::config::ContentVersion>,
    #[serde(default)]
    script: Option<arena::script::ScriptVersion>,
    #[serde(default)]
    bridge: embedded::BridgeConfig,
    #[serde(default)]
    storage_directory: Option<PathBuf>,
    #[serde(default)]
    content_path: Option<PathBuf>,
    #[serde(default)]
    script_path: Option<PathBuf>,
}
fn contract_version() -> u32 {
    arena::SCHEMA_VERSION
}

fn factory(bytes: &[u8]) -> Result<Box<dyn ApplicationDriver>, String> {
    let config: NativeConfig = serde_json::from_slice(if bytes.is_empty() { b"{}" } else { bytes })
        .map_err(|error| format!("invalid Arena native configuration: {error}"))?;
    if config.contract_version != arena::SCHEMA_VERSION {
        return Err(format!(
            "Arena contract version must be {}",
            arena::SCHEMA_VERSION
        ));
    }
    let runtime = if config.content.is_some() || config.script.is_some() {
        let content = match config.content {
            Some(content) => content,
            None => {
                arena::config::ContentVersion::from_tmd(include_bytes!("../../content/arena.tmd"))
                    .map_err(|error| error.to_string())?
            }
        };
        let script = match config.script {
            Some(script) => script,
            None => arena::script::default_script().map_err(|error| error.to_string())?,
        };
        content.validate().map_err(|error| error.to_string())?;
        let mut runtime = build_demo_runtime().map_err(|error| error.to_string())?;
        arena::install_with_versions(&mut runtime, content, script)
            .map_err(|error| error.to_string())?;
        runtime
    } else {
        build_arena_runtime().map_err(|error| error.to_string())?
    };
    Ok(Box::new(embedded::EmbeddedDriver::new(
        runtime,
        config.bridge,
        config.storage_directory,
        config.content_path,
        config.script_path,
    )?))
}

/// Returns the supported C ABI version without creating a Runtime.
///
/// # Examples
/// ```
/// assert_eq!(kitu_demo_game_native::kitu_application_abi_version(), 1);
/// ```
#[no_mangle]
pub extern "C" fn kitu_application_abi_version() -> u32 {
    ffi::ABI_VERSION
}

/// Creates an unstarted Arena, using embedded TMD or detached validated content.
/// Empty configuration means defaults. Failures return no handle and report the
/// required diagnostic length without truncating into short buffers.
///
/// # Safety
/// All non-null pointers must be valid for their declared lengths; output pointers
/// must be writable and disjoint. See the shared header for exact buffer rules.
#[no_mangle]
pub unsafe extern "C" fn kitu_application_create(
    requested_abi: u32,
    config: *const u8,
    config_len: usize,
    out_handle: *mut *mut ApplicationHandle,
    error_buffer: *mut u8,
    error_capacity: usize,
    out_error_required: *mut usize,
) -> i32 {
    ffi::create_json(
        requested_abi,
        config,
        config_len,
        out_handle,
        error_buffer,
        error_capacity,
        out_error_required,
        factory,
    )
}

/// Destroys a live handle exactly once on its owning thread.
///
/// # Safety
/// `handle` must be null or a still-live pointer returned by creation. No call may
/// race with destruction; the pointer must never be used after OK or PANIC.
#[no_mangle]
pub unsafe extern "C" fn kitu_application_destroy(handle: *mut ApplicationHandle) -> i32 {
    ffi::destroy(handle)
}

/// Admits one typed JSON bundle without advancing the clock.
/// Success returns its queue sequence; game acceptance is an emitted receipt.
///
/// # Safety
/// The live handle belongs to this thread. Input and output pointers are valid,
/// non-overlapping and remain alive for the call. Null input is valid only at zero length.
#[no_mangle]
pub unsafe extern "C" fn kitu_application_submit_json(
    handle: *mut ApplicationHandle,
    input: *const u8,
    input_len: usize,
    out_sequence: *mut u64,
) -> i32 {
    ffi::submit_json(handle, input, input_len, out_sequence)
}

/// Advances the same application Runtime once at 60 Hz logical time.
/// Pending output must be retrieved before another tick; no wall clock is read.
///
/// # Safety
/// `handle` must be null or a live handle owned by the calling thread.
#[no_mangle]
pub unsafe extern "C" fn kitu_application_tick(handle: *mut ApplicationHandle) -> i32 {
    ffi::tick(handle)
}

/// Retrieves the entire ordered JSON output batch from the last tick.
/// A length query or short buffer does not consume output.
///
/// # Safety
/// The handle must be live and owned by this thread. The writable buffer and
/// required-length pointer must be valid, disjoint, and not alias any handle data.
#[no_mangle]
pub unsafe extern "C" fn kitu_application_read_output(
    handle: *mut ApplicationHandle,
    buffer: *mut u8,
    capacity: usize,
    out_required: *mut usize,
) -> i32 {
    ffi::read_output(handle, buffer, capacity, out_required)
}

/// Inspects detached application projection bundles without consuming events.
///
/// # Safety
/// The live handle is owned by this thread. Buffer and length output pointers
/// must be valid, disjoint, and follow the shared header's capacity rules.
#[no_mangle]
pub unsafe extern "C" fn kitu_application_inspect_json(
    handle: *mut ApplicationHandle,
    buffer: *mut u8,
    capacity: usize,
    out_required: *mut usize,
) -> i32 {
    ffi::inspect_json(handle, buffer, capacity, out_required)
}

/// Inspects host/session metadata without changing deterministic game projections.
///
/// # Safety
/// The live handle is owned by this thread. Buffer and length output pointers
/// must be valid, disjoint, and follow the shared header's capacity rules.
#[no_mangle]
pub unsafe extern "C" fn kitu_application_inspect_host_json(
    handle: *mut ApplicationHandle,
    buffer: *mut u8,
    capacity: usize,
    out_required: *mut usize,
) -> i32 {
    ffi::inspect_host_json(handle, buffer, capacity, out_required)
}

/// Reads the latest diagnostic without truncation or clearing it.
///
/// # Safety
/// The live handle is owned by this thread. Buffer and length output pointers
/// must be valid, disjoint, and follow the shared header's capacity rules.
#[no_mangle]
pub unsafe extern "C" fn kitu_application_last_error(
    handle: *mut ApplicationHandle,
    buffer: *mut u8,
    capacity: usize,
    out_required: *mut usize,
) -> i32 {
    ffi::last_error(handle, buffer, capacity, out_required)
}

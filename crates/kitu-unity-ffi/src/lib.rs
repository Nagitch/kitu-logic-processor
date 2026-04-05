//! FFI boundary for embedding the runtime in Unity.
//!
//! # Responsibilities
//! - Expose a C-compatible surface area for Unity callers while preserving Rust-side safety.
//! - Marshal buffers and event payloads between Unity and the runtime without leaking abstractions.
//! - Document invariants and threading requirements for embedders.
//!
//! # Integration
//! This crate wraps the runtime (`kitu-runtime`) and transports (`kitu-transport`) behind a
//! Unity-friendly API. See `doc/crates-overview.md` for how the FFI sits atop the core runtime.

use std::{
    collections::VecDeque,
    ffi::CStr,
    os::raw::c_char,
    sync::{Arc, Mutex},
};

use kitu_core::Result;
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::{build_runtime, Runtime};
use kitu_transport::LocalChannel;

const MAX_ENTITY_ID_BYTES: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct RenderTransformEvent {
    pub entity_id: String,
    pub tick: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KituRenderTransformEvent {
    pub entity_id_len: u32,
    pub entity_id: [u8; MAX_ENTITY_ID_BYTES],
    pub tick: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl KituRenderTransformEvent {
    fn from_event(event: &RenderTransformEvent) -> Option<Self> {
        let bytes = event.entity_id.as_bytes();
        if bytes.len() > MAX_ENTITY_ID_BYTES {
            return None;
        }

        let mut entity_id = [0_u8; MAX_ENTITY_ID_BYTES];
        entity_id[..bytes.len()].copy_from_slice(bytes);
        Some(Self {
            entity_id_len: u32::try_from(bytes.len()).ok()?,
            entity_id,
            tick: event.tick,
            x: event.x,
            y: event.y,
            z: event.z,
        })
    }
}

/// Managed handle exposed to Unity.
#[derive(Clone)]
pub struct UnityHandle {
    runtime: Arc<Mutex<Runtime<LocalChannel>>>,
    pending_render_events: Arc<Mutex<VecDeque<RenderTransformEvent>>>,
}

impl UnityHandle {
    /// Initializes the runtime and returns a handle safe to share across FFI boundaries.
    pub fn initialize() -> Self {
        let runtime = build_runtime(LocalChannel::connected());
        Self {
            runtime: Arc::new(Mutex::new(runtime)),
            pending_render_events: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Queues one `/input/move` command for runtime-owned processing on the next tick.
    pub fn submit_move_input(&self, entity_id: &str, x: f32, y: f32) -> bool {
        if entity_id.len() > MAX_ENTITY_ID_BYTES {
            return false;
        }

        let mut message = OscMessage::new("/input/move");
        message.push_arg(OscArg::Str(entity_id.to_string()));
        message.push_arg(OscArg::Float(x));
        message.push_arg(OscArg::Float(y));

        let mut bundle = OscBundle::new();
        bundle.push(message);

        let mut guard = self.runtime.lock().expect("runtime mutex poisoned");
        guard.enqueue_input(bundle);
        true
    }

    /// Advances the runtime by one tick.
    pub fn tick(&self) -> Result<()> {
        let mut guard = self.runtime.lock().expect("runtime mutex poisoned");
        guard.tick_once()
    }

    /// Pops one `/render/player/transform` event emitted by the runtime output path.
    pub fn pop_render_transform(&self) -> Option<RenderTransformEvent> {
        if let Some(event) = self
            .pending_render_events
            .lock()
            .expect("render queue mutex poisoned")
            .pop_front()
        {
            return Some(event);
        }

        let mut runtime = self.runtime.lock().expect("runtime mutex poisoned");
        let outputs = runtime.drain_output_buffer();
        drop(runtime);

        let mut pending = self
            .pending_render_events
            .lock()
            .expect("render queue mutex poisoned");
        for bundle in outputs {
            if let Some(event) = parse_render_transform(bundle) {
                pending.push_back(event);
            }
        }
        pending.pop_front()
    }
}

/// C ABI entry point to create a new runtime handle.
#[no_mangle]
pub extern "C" fn kitu_init() -> *mut UnityHandle {
    Box::into_raw(Box::new(UnityHandle::initialize()))
}

/// C ABI entry point to advance the runtime.
///
/// # Safety
///
/// - `handle` must be a valid pointer created by [`kitu_init`].
/// - The pointed-to handle must not be freed while this function runs.
/// - Callers must ensure the pointer is not shared across threads without
///   external synchronization.
#[no_mangle]
pub unsafe extern "C" fn kitu_tick(handle: *mut UnityHandle) -> i32 {
    let Some(handle) = handle.as_ref() else {
        return -1;
    };
    match handle.tick() {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

/// C ABI entry point to submit one move input.
///
/// # Safety
///
/// - `handle` must be a valid pointer created by [`kitu_init`].
/// - `entity_id` must be a valid, NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn kitu_submit_move_input(
    handle: *mut UnityHandle,
    entity_id: *const c_char,
    x: f32,
    y: f32,
) -> i32 {
    let Some(handle) = handle.as_ref() else {
        return -1;
    };
    if entity_id.is_null() {
        return -2;
    }
    let Ok(entity_id) = CStr::from_ptr(entity_id).to_str() else {
        return -3;
    };
    if !handle.submit_move_input(entity_id, x, y) {
        return -4;
    }
    0
}

/// C ABI entry point to pop one render transform event.
///
/// Returns:
/// - `1` when an event is written to `out_event`
/// - `0` when no event is available
/// - negative values on invalid pointers or encoding failures
///
/// # Safety
///
/// - `handle` must be a valid pointer created by [`kitu_init`].
/// - `out_event` must be valid for writes.
#[no_mangle]
pub unsafe extern "C" fn kitu_pop_render_transform(
    handle: *mut UnityHandle,
    out_event: *mut KituRenderTransformEvent,
) -> i32 {
    let Some(handle) = handle.as_ref() else {
        return -1;
    };
    let Some(out_event) = out_event.as_mut() else {
        return -2;
    };
    let Some(event) = handle.pop_render_transform() else {
        return 0;
    };
    let Some(encoded) = KituRenderTransformEvent::from_event(&event) else {
        return -3;
    };
    *out_event = encoded;
    1
}

fn parse_render_transform(bundle: OscBundle) -> Option<RenderTransformEvent> {
    let message = bundle
        .messages
        .into_iter()
        .find(|m| m.address == "/render/player/transform")?;
    if message.args.len() != 5 {
        return None;
    }
    let entity_id = match &message.args[0] {
        OscArg::Str(value) => value.clone(),
        _ => return None,
    };
    let tick = match message.args[1] {
        OscArg::Int(value) => u64::try_from(value).ok()?,
        OscArg::Int64(value) => u64::try_from(value).ok()?,
        _ => return None,
    };
    let x = match message.args[2] {
        OscArg::Float(value) => value,
        _ => return None,
    };
    let y = match message.args[3] {
        OscArg::Float(value) => value,
        _ => return None,
    };
    let z = match message.args[4] {
        OscArg::Float(value) => value,
        _ => return None,
    };

    Some(RenderTransformEvent {
        entity_id,
        tick,
        x,
        y,
        z,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn handle_advances_runtime() {
        let handle = UnityHandle::initialize();
        handle.tick().unwrap();
        handle.tick().unwrap();
        let runtime = handle.runtime.lock().unwrap();
        assert_eq!(runtime.current_tick().get(), 2);
    }

    #[test]
    fn ffi_functions_return_success() {
        let ptr = kitu_init();
        let status = unsafe { kitu_tick(ptr) };
        assert_eq!(status, 0);
        unsafe { drop(Box::from_raw(ptr)) };
    }

    #[test]
    fn boundary_smoke_executes_move_slice() {
        let handle = UnityHandle::initialize();
        assert!(handle.submit_move_input("player-1", 1.5, -2.0));
        handle.tick().unwrap();

        let event = handle
            .pop_render_transform()
            .expect("expected render event");
        assert_eq!(event.entity_id, "player-1");
        assert_eq!(event.tick, 0);
        assert_eq!(event.x, 1.5);
        assert_eq!(event.y, -2.0);
        assert_eq!(event.z, 0.0);
    }

    #[test]
    fn ffi_boundary_smoke_executes_move_slice() {
        let ptr = kitu_init();
        let entity_id = CString::new("ffi-player").unwrap();
        let submit_status = unsafe { kitu_submit_move_input(ptr, entity_id.as_ptr(), 2.0, 3.0) };
        assert_eq!(submit_status, 0);
        assert_eq!(unsafe { kitu_tick(ptr) }, 0);

        let mut out = KituRenderTransformEvent {
            entity_id_len: 0,
            entity_id: [0; MAX_ENTITY_ID_BYTES],
            tick: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        let pop_status = unsafe { kitu_pop_render_transform(ptr, &mut out as *mut _) };
        assert_eq!(pop_status, 1);
        assert_eq!(out.tick, 0);
        assert_eq!(out.x, 2.0);
        assert_eq!(out.y, 3.0);
        assert_eq!(out.z, 0.0);
        assert_eq!(
            std::str::from_utf8(&out.entity_id[..out.entity_id_len as usize]).unwrap(),
            "ffi-player"
        );

        unsafe { drop(Box::from_raw(ptr)) };
    }

    #[test]
    fn ffi_submit_rejects_oversized_entity_id() {
        let ptr = kitu_init();
        let oversized_id = CString::new("x".repeat(MAX_ENTITY_ID_BYTES + 1)).unwrap();
        let status = unsafe { kitu_submit_move_input(ptr, oversized_id.as_ptr(), 0.0, 0.0) };
        assert_eq!(status, -4);
        unsafe { drop(Box::from_raw(ptr)) };
    }
}

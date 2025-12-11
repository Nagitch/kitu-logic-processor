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

use std::sync::{Arc, Mutex};

use kitu_core::Result;
use kitu_runtime::{build_runtime, Runtime};
use kitu_transport::LocalChannel;

/// Managed handle exposed to Unity.
#[derive(Clone)]
pub struct UnityHandle {
    runtime: Arc<Mutex<Runtime<LocalChannel>>>,
}

impl UnityHandle {
    /// Initializes the runtime and returns a handle safe to share across FFI boundaries.
    pub fn initialize() -> Self {
        let runtime = build_runtime(LocalChannel::connected());
        Self {
            runtime: Arc::new(Mutex::new(runtime)),
        }
    }

    /// Advances the runtime by one tick.
    pub fn tick(&self) -> Result<()> {
        let mut guard = self.runtime.lock().expect("runtime mutex poisoned");
        guard.tick_once()
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
    let Some(handle) = handle.as_mut() else {
        return -1;
    };
    match handle.tick() {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

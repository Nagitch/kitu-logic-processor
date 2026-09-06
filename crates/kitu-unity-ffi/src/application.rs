//! Full-application, thread-affine C ABI helpers for application-owned libraries.
//!
//! Applications export thin `extern "C"` wrappers using `include/kitu_application.h`.
//! Their factory installs the same application used by the server; this crate has
//! no game dependency. Typed JSON preserves OSC widths, bundle and message order.
//! A successful submit acknowledges admission. Application acceptance or refusal
//! is a later output from the ordinary authoritative tick.
//!
//! # Examples
//! ```
//! use kitu_runtime::build_runtime;
//! use kitu_transport::LocalChannel;
//! use kitu_unity_ffi::application::{self, ApplicationHandle, RuntimeDriver};
//! let runtime = build_runtime(LocalChannel::connected());
//! let handle = Box::into_raw(Box::new(ApplicationHandle::new(Box::new(
//!     RuntimeDriver::new(runtime),
//! ))));
//! unsafe {
//!     assert_eq!(application::tick(handle), application::OK);
//!     let mut required = 0;
//!     assert_eq!(application::read_output(handle, std::ptr::null_mut(), 0, &mut required),
//!         application::BUFFER_TOO_SMALL);
//!     let mut bytes = vec![0; required];
//!     assert_eq!(application::read_output(handle, bytes.as_mut_ptr(), bytes.len(), &mut required),
//!         application::OK);
//!     assert_eq!(application::destroy(handle), application::OK);
//! }
//! ```

use std::{
    io::{self, Write},
    panic::{catch_unwind, AssertUnwindSafe},
    ptr,
    thread::{self, ThreadId},
};

use kitu_osc_ir::OscBundle;
use kitu_runtime::{InputMetadata, Runtime};
use kitu_transport::{
    wire::{WireBundle, WireBundlesRef},
    LocalChannel,
};
use serde::{Deserialize, Serialize};

/// C ABI version; application contract versions are carried separately in metadata.
pub const ABI_VERSION: u32 = 1;
/// The operation completed; submission means queued, not application acceptance.
pub const OK: i32 = 0;
/// No pending output batch exists.
pub const EMPTY: i32 = 1;
/// The required length was returned and no bytes or output ownership changed.
pub const BUFFER_TOO_SMALL: i32 = 2;
/// A required pointer or pointer/length combination was invalid.
pub const INVALID_ARGUMENT: i32 = -1;
/// Input JSON, metadata or typed OSC was invalid.
pub const INVALID_INPUT: i32 = -2;
/// This call came from a thread other than the handle's creating thread.
pub const WRONG_THREAD: i32 = -3;
/// Read the previous tick's complete output batch before advancing again.
pub const PENDING_OUTPUT: i32 = -4;
/// A previous panic or failed tick permanently disabled this handle.
pub const FAILED: i32 = -5;
/// A Rust panic was caught; the existing handle is permanently disabled.
pub const PANIC: i32 = -6;
/// The per-tick request count or serialized input byte budget was exhausted.
pub const QUEUE_FULL: i32 = -7;
/// The application driver rejected the request or could not complete the operation.
pub const DRIVER_ERROR: i32 = -8;
/// The caller requested an unsupported C ABI version.
pub const ABI_MISMATCH: i32 = -9;

/// Maximum bytes in one submitted JSON request or factory configuration (1 MiB).
pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
/// Maximum requests admitted between successful ticks.
pub const MAX_PENDING_INPUTS: usize = 4096;
/// Maximum cumulative JSON request bytes admitted between ticks (16 MiB).
pub const MAX_PENDING_INPUT_BYTES: usize = 16 * 1024 * 1024;
/// Maximum serialized output or inspection batch (64 MiB).
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

/// A host-independent application interface, invoked exclusively by its owner thread.
///
/// Drivers must use the application's normal ordered input queue and tick path.
/// They must not create a competing timer or execute inputs during submission.
/// Driver panics are caught at the ABI boundary. An error from `tick` is fatal
/// because a driver may already have partially advanced.
pub trait ApplicationDriver: Send {
    /// Validates and queues one bundle, returning its authoritative admission order.
    ///
    /// On error, the driver must leave its input queue and state unchanged.
    fn submit(&mut self, bundle: OscBundle, metadata: Option<InputMetadata>)
        -> Result<u64, String>;

    /// Advances exactly one management tick and returns all outputs in emission order.
    fn tick(&mut self) -> Result<Vec<OscBundle>, String>;

    /// Returns a detached application projection without advancing or mutating state.
    fn inspect(&self) -> Result<Vec<OscBundle>, String>;

    /// Returns optional host/session metadata separately from deterministic game state.
    ///
    /// The default is an empty batch. Tools may inspect session identity or playback
    /// mode here without adding transport details to application output or replay proofs.
    /// This method must not advance the clock, consume output, or mutate the host.
    fn inspect_host(&self) -> Result<Vec<OscBundle>, String> {
        Ok(Vec::new())
    }
}

/// An adapter around an ordinary local runtime with an application installed by its owner.
pub struct RuntimeDriver {
    runtime: Runtime<LocalChannel>,
}

impl RuntimeDriver {
    /// Takes exclusive ownership of a runtime; no second tick loop is started.
    ///
    /// # Examples
    /// ```
    /// use kitu_runtime::build_runtime;
    /// use kitu_transport::LocalChannel;
    /// use kitu_unity_ffi::application::{ApplicationDriver, RuntimeDriver};
    /// let mut driver = RuntimeDriver::new(build_runtime(LocalChannel::connected()));
    /// driver.tick().unwrap();
    /// assert!(driver.inspect().unwrap().is_empty());
    /// ```
    pub fn new(runtime: Runtime<LocalChannel>) -> Self {
        Self { runtime }
    }
}

impl ApplicationDriver for RuntimeDriver {
    fn submit(
        &mut self,
        bundle: OscBundle,
        metadata: Option<InputMetadata>,
    ) -> Result<u64, String> {
        self.runtime
            .try_enqueue_input(bundle, metadata)
            .map_err(|error| error.to_string())
    }

    fn tick(&mut self) -> Result<Vec<OscBundle>, String> {
        self.runtime
            .tick_once()
            .map_err(|error| error.to_string())?;
        let output = self.runtime.drain_output_buffer();
        self.runtime.drain_committed_inputs();
        Ok(output)
    }

    fn inspect(&self) -> Result<Vec<OscBundle>, String> {
        Ok(self.runtime.inspect_application())
    }
}

/// Typed JSON request accepted by [`submit_json`]. Unknown fields are rejected.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputRequest {
    /// Producer identity and application contract version; legacy inputs may omit it.
    pub metadata: Option<InputMetadata>,
    /// Ordered OSC messages and explicitly typed arguments.
    pub bundle: WireBundle,
}

/// Opaque C handle; its layout and Rust allocations are never exposed to the caller.
///
/// All calls, including destruction, must be sequential on the creating thread.
/// Callers own all byte buffers. No Rust allocation must be freed by another
/// allocator. The only transfer of ownership is `create_json` / `destroy`.
pub struct ApplicationHandle {
    driver: Box<dyn ApplicationDriver>,
    owner: ThreadId,
    pending_output: Option<Vec<u8>>,
    pending_inputs: usize,
    pending_input_bytes: usize,
    last_error: String,
    failed: bool,
}

impl ApplicationHandle {
    /// Wraps an application driver and binds all future calls to this thread.
    ///
    /// See the module example for converting this value to an opaque pointer.
    pub fn new(driver: Box<dyn ApplicationDriver>) -> Self {
        Self {
            driver,
            owner: thread::current().id(),
            pending_output: None,
            pending_inputs: 0,
            pending_input_bytes: 0,
            last_error: String::new(),
            failed: false,
        }
    }

    fn error(&mut self, status: i32, message: impl Into<String>) -> i32 {
        self.last_error = message.into();
        status
    }
}

/// Creates a handle through an application-owned factory with bounded configuration.
///
/// `out_handle` is cleared before invoking the factory and remains null on every
/// failure. An empty configuration is legal; the factory defines its meaning.
/// Failure diagnostics use caller-owned storage: `out_error_required` always
/// receives the full UTF-8 byte count, and bytes are written only when they all
/// fit. The original error status is preserved when that buffer is too small.
/// Repeating failed creation with a larger buffer creates no duplicate handle.
///
/// # Safety
/// Pointer/length pairs must refer to readable configuration and writable output
/// storage for their complete stated lengths. Null byte pointers require zero
/// length/capacity. `out_handle` and `out_error_required` must be valid, aligned,
/// writable pointers. All referenced storage must be disjoint and remain valid
/// until this call returns. The factory must return exclusively owned state.
#[allow(clippy::too_many_arguments)]
pub unsafe fn create_json(
    requested_abi: u32,
    config: *const u8,
    config_length: usize,
    out_handle: *mut *mut ApplicationHandle,
    error_buffer: *mut u8,
    error_capacity: usize,
    out_error_required: *mut usize,
    factory: impl FnOnce(&[u8]) -> Result<Box<dyn ApplicationDriver>, String>,
) -> i32 {
    let diagnostic_valid = valid_destination(error_buffer, error_capacity, out_error_required);
    if !out_handle.is_null() {
        ptr::write(out_handle, ptr::null_mut());
    }
    if diagnostic_valid {
        ptr::write(out_error_required, 0);
    }
    let outcome = catch_unwind(AssertUnwindSafe(move || {
        // Keep the factory's captured values inside the unwind boundary even
        // when invalid arguments prevent invoking it: their Drop may panic.
        if out_handle.is_null() || !diagnostic_valid {
            return Err((
                INVALID_ARGUMENT,
                "invalid creation output pointers".to_string(),
            ));
        }
        if requested_abi != ABI_VERSION {
            return Err((ABI_MISMATCH, "unsupported C ABI version".to_string()));
        }
        let config = checked_bytes(config, config_length).map_err(|status| {
            (
                status,
                "invalid configuration pointer or length".to_string(),
            )
        })?;
        let driver = factory(config).map_err(|message| (DRIVER_ERROR, message))?;
        Ok(Box::into_raw(Box::new(ApplicationHandle::new(driver))))
    }));
    match outcome {
        Ok(Ok(handle)) => {
            ptr::write(out_handle, handle);
            OK
        }
        Ok(Err((status, message))) => {
            if diagnostic_valid {
                copy_bytes(
                    message.as_bytes(),
                    error_buffer,
                    error_capacity,
                    out_error_required,
                );
            }
            status
        }
        Err(payload) => {
            // A user-provided panic payload may itself panic during Drop.
            std::mem::forget(payload);
            if diagnostic_valid {
                copy_bytes(
                    b"application factory panicked",
                    error_buffer,
                    error_capacity,
                    out_error_required,
                );
            }
            PANIC
        }
    }
}

/// Destroys exactly one owned handle; null is invalid and double destruction is UB.
///
/// A failed handle can still be destroyed on its owner thread. A caught destructor
/// panic returns [`PANIC`], but ownership is consumed in that case too.
///
/// # Safety
/// `handle` must be null or a live pointer returned by `create_json` (or an owned
/// `Box<ApplicationHandle>` converted with `Box::into_raw`). No other call may
/// overlap. Never use the pointer again after this returns `OK` or `PANIC`.
pub unsafe fn destroy(handle: *mut ApplicationHandle) -> i32 {
    let Some(reference) = handle.as_ref() else {
        return INVALID_ARGUMENT;
    };
    if reference.owner != thread::current().id() {
        return WRONG_THREAD;
    }
    match catch_unwind(AssertUnwindSafe(|| drop(Box::from_raw(handle)))) {
        Ok(()) => OK,
        Err(payload) => {
            std::mem::forget(payload);
            PANIC
        }
    }
}

/// Admits typed JSON to the next tick's ordered queue and writes its sequence.
///
/// Validating arguments, JSON and metadata precedes driver submission. Queued
/// inputs are limited by [`MAX_PENDING_INPUTS`] and [`MAX_PENDING_INPUT_BYTES`].
/// Refusal leaves `out_sequence` unchanged. Application acceptance is reported
/// by a later output receipt, not by this function's admission status.
///
/// # Safety
/// The handle must satisfy [`destroy`]'s live, exclusive ownership requirements.
/// `input` must reference `length` readable bytes (null only when length is zero).
/// `out_sequence` must be aligned and writable. Storage must be disjoint from
/// the handle and input, with no concurrent access until this call returns.
pub unsafe fn submit_json(
    handle: *mut ApplicationHandle,
    input: *const u8,
    length: usize,
    out_sequence: *mut u64,
) -> i32 {
    with_handle(handle, false, |handle| {
        if out_sequence.is_null() {
            return handle.error(INVALID_ARGUMENT, "out_sequence must not be null");
        }
        let bytes = match checked_bytes(input, length) {
            Ok(bytes) => bytes,
            Err(status) => return handle.error(status, "invalid input pointer or length"),
        };
        if handle.pending_inputs >= MAX_PENDING_INPUTS
            || length > MAX_PENDING_INPUT_BYTES - handle.pending_input_bytes
        {
            return handle.error(QUEUE_FULL, "pending input budget exhausted; advance a tick");
        }
        let request: InputRequest = match serde_json::from_slice(bytes) {
            Ok(request) => request,
            Err(error) => {
                return handle.error(INVALID_INPUT, format!("invalid input JSON: {error}"))
            }
        };
        if let Some(metadata) = &request.metadata {
            if metadata.source.is_empty()
                || metadata.source.len() > 128
                || metadata.source.contains('\0')
                || metadata.message_id == 0
                || metadata.schema_version == 0
            {
                return handle.error(INVALID_INPUT, "metadata requires source 1..128 bytes without NUL, positive messageId and schemaVersion");
            }
        }
        let bundle = match OscBundle::try_from(request.bundle) {
            Ok(bundle) => bundle,
            Err(error) => return handle.error(INVALID_INPUT, error.to_string()),
        };
        match handle.driver.submit(bundle, request.metadata) {
            Ok(sequence) => {
                handle.pending_inputs += 1;
                handle.pending_input_bytes += length;
                ptr::write(out_sequence, sequence);
                OK
            }
            Err(message) => handle.error(DRIVER_ERROR, message),
        }
    })
}

/// Advances exactly one application tick, retaining one complete JSON output batch.
///
/// A pending batch causes [`PENDING_OUTPUT`] before any state change. A tick with
/// no events still produces `[]`, which must be drained. Driver or encoding
/// errors permanently disable the handle because the tick may have advanced.
///
/// # Safety
/// The handle must satisfy [`destroy`]'s live, exclusive ownership requirements.
pub unsafe fn tick(handle: *mut ApplicationHandle) -> i32 {
    with_handle(handle, false, |handle| {
        if handle.pending_output.is_some() {
            return handle.error(
                PENDING_OUTPUT,
                "read the previous tick output before advancing",
            );
        }
        let result = handle.driver.tick().and_then(encode_bundles);
        match result {
            Ok(bytes) => {
                handle.pending_output = Some(bytes);
                handle.pending_inputs = 0;
                handle.pending_input_bytes = 0;
                OK
            }
            Err(message) => {
                handle.failed = true;
                handle.error(DRIVER_ERROR, message)
            }
        }
    })
}

/// Reads and consumes one whole tick batch after a successful complete copy.
///
/// A zero-capacity query or short buffer returns [`BUFFER_TOO_SMALL`] and retains
/// every byte. `out_required` receives the exact count, excluding any NUL byte.
/// After consumption a repeated read returns [`EMPTY`] with length zero.
///
/// # Safety
/// The handle must satisfy [`destroy`]'s live, exclusive ownership requirements.
/// The destination must satisfy [`inspect_json`]'s output-buffer requirements.
pub unsafe fn read_output(
    handle: *mut ApplicationHandle,
    buffer: *mut u8,
    capacity: usize,
    out_required: *mut usize,
) -> i32 {
    with_handle(handle, false, |handle| {
        if !valid_destination(buffer, capacity, out_required) {
            return handle.error(
                INVALID_ARGUMENT,
                "invalid output buffer or required-length pointer",
            );
        }
        let Some(bytes) = &handle.pending_output else {
            ptr::write(out_required, 0);
            return EMPTY;
        };
        let result = copy_bytes(bytes, buffer, capacity, out_required);
        if result == OK {
            handle.pending_output = None;
        }
        result
    })
}

/// Copies a detached typed-JSON projection without consuming output or advancing.
///
/// The projection is a JSON array of ordered `WireBundle` objects. Length queries
/// use a null buffer with capacity zero. Short buffers receive no partial bytes.
///
/// # Safety
/// The handle must satisfy [`destroy`]'s live, exclusive ownership requirements.
/// `buffer` must be writable for `capacity` bytes; null requires capacity zero.
/// `out_required` must be non-null, aligned and writable. These allocations must
/// not overlap each other or the handle, and must remain exclusively accessible
/// until this function returns. No buffer is retained by Rust.
pub unsafe fn inspect_json(
    handle: *mut ApplicationHandle,
    buffer: *mut u8,
    capacity: usize,
    out_required: *mut usize,
) -> i32 {
    inspect_with(handle, buffer, capacity, out_required, false)
}

/// Copies detached host/session metadata using the same bounded typed-JSON format.
///
/// This additive ABI 1 operation keeps nondeterministic host identity out of game
/// projections and recorded output. Drivers without host metadata return `[]`.
/// Length queries and short reads never consume output or advance the clock.
///
/// # Safety
/// The handle and destination must satisfy [`inspect_json`]'s requirements.
pub unsafe fn inspect_host_json(
    handle: *mut ApplicationHandle,
    buffer: *mut u8,
    capacity: usize,
    out_required: *mut usize,
) -> i32 {
    inspect_with(handle, buffer, capacity, out_required, true)
}

unsafe fn inspect_with(
    handle: *mut ApplicationHandle,
    buffer: *mut u8,
    capacity: usize,
    out_required: *mut usize,
    host_metadata: bool,
) -> i32 {
    with_handle(handle, false, |handle| {
        if !valid_destination(buffer, capacity, out_required) {
            return handle.error(
                INVALID_ARGUMENT,
                "invalid inspection buffer or required-length pointer",
            );
        }
        let projection = if host_metadata {
            handle.driver.inspect_host()
        } else {
            handle.driver.inspect()
        };
        match projection.and_then(encode_bundles) {
            Ok(bytes) => copy_bytes(&bytes, buffer, capacity, out_required),
            Err(message) => handle.error(DRIVER_ERROR, message),
        }
    })
}

/// Copies the last diagnostic, including on a permanently failed handle.
///
/// The text is UTF-8 with no NUL terminator; it persists until another error.
/// Buffer queries never replace it. Initially the diagnostic is empty (`OK`,
/// required length zero). A wrong-thread call cannot inspect or change it.
///
/// # Safety
/// The handle and destination must satisfy [`inspect_json`]'s requirements.
pub unsafe fn last_error(
    handle: *mut ApplicationHandle,
    buffer: *mut u8,
    capacity: usize,
    out_required: *mut usize,
) -> i32 {
    with_handle(handle, true, |handle| {
        if !valid_destination(buffer, capacity, out_required) {
            return INVALID_ARGUMENT;
        }
        copy_bytes(handle.last_error.as_bytes(), buffer, capacity, out_required)
    })
}

unsafe fn with_handle(
    pointer: *mut ApplicationHandle,
    allow_failed: bool,
    operation: impl FnOnce(&mut ApplicationHandle) -> i32,
) -> i32 {
    let Some(handle) = pointer.as_mut() else {
        return INVALID_ARGUMENT;
    };
    if handle.owner != thread::current().id() {
        return WRONG_THREAD;
    }
    if handle.failed && !allow_failed {
        return FAILED;
    }
    match catch_unwind(AssertUnwindSafe(|| operation(handle))) {
        Ok(status) => status,
        Err(payload) => {
            std::mem::forget(payload);
            handle.failed = true;
            handle.last_error = "application driver panicked; destroy this handle".to_string();
            PANIC
        }
    }
}

unsafe fn checked_bytes<'a>(pointer: *const u8, length: usize) -> Result<&'a [u8], i32> {
    if length > MAX_INPUT_BYTES {
        return Err(INVALID_INPUT);
    }
    if pointer.is_null() && length != 0 {
        return Err(INVALID_ARGUMENT);
    }
    if length == 0 {
        Ok(&[])
    } else {
        Ok(std::slice::from_raw_parts(pointer, length))
    }
}

fn valid_destination(buffer: *mut u8, capacity: usize, required: *mut usize) -> bool {
    !required.is_null() && (!buffer.is_null() || capacity == 0) && capacity <= isize::MAX as usize
}

unsafe fn copy_bytes(bytes: &[u8], buffer: *mut u8, capacity: usize, required: *mut usize) -> i32 {
    ptr::write(required, bytes.len());
    if capacity < bytes.len() {
        return BUFFER_TOO_SMALL;
    }
    if !bytes.is_empty() {
        ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
    }
    OK
}

fn encode_bundles(bundles: Vec<OscBundle>) -> Result<Vec<u8>, String> {
    let mut writer = LimitedBuffer(Vec::new());
    serde_json::to_writer(&mut writer, &WireBundlesRef::new(&bundles))
        .map_err(|error| error.to_string())?;
    Ok(writer.0)
}

struct LimitedBuffer(Vec<u8>);

impl Write for LimitedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_OUTPUT_BYTES - self.0.len() {
            return Err(io::Error::other("output exceeds 64 MiB ABI limit"));
        }
        let required = self.0.len() + bytes.len();
        if required > self.0.capacity() {
            // Preserve amortized growth without allowing Vec's next doubling
            // to reserve beyond the output limit before serialization stops.
            let capacity = required
                .max(self.0.capacity().max(256) * 2)
                .min(MAX_OUTPUT_BYTES);
            self.0
                .try_reserve_exact(capacity - self.0.len())
                .map_err(io::Error::other)?;
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitu_osc_ir::{OscArg, OscMessage};
    use kitu_runtime::build_runtime;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct Probe {
        count: u64,
        submissions: usize,
        drops: Arc<AtomicUsize>,
        panic_tick: bool,
        fail_tick: bool,
    }

    impl ApplicationDriver for Probe {
        fn submit(&mut self, bundle: OscBundle, _: Option<InputMetadata>) -> Result<u64, String> {
            if bundle
                .messages
                .iter()
                .any(|message| message.address == "/reject")
            {
                return Err("structural rejection".to_string());
            }
            let sequence = self.submissions as u64;
            self.submissions += 1;
            Ok(sequence)
        }

        fn tick(&mut self) -> Result<Vec<OscBundle>, String> {
            self.count += 1;
            assert!(!self.panic_tick, "tick panicked");
            if self.fail_tick {
                return Err("partially advanced tick failed".to_string());
            }
            self.inspect()
        }

        fn inspect(&self) -> Result<Vec<OscBundle>, String> {
            let mut message = OscMessage::new("/test/state");
            message.push_arg(OscArg::Int64(self.count as i64));
            message.push_arg(OscArg::Int(self.submissions as i32));
            let mut bundle = OscBundle::new();
            bundle.push(message);
            Ok(vec![bundle])
        }
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn probe(panic_tick: bool, fail_tick: bool) -> (Box<dyn ApplicationDriver>, Arc<AtomicUsize>) {
        let drops = Arc::new(AtomicUsize::new(0));
        (
            Box::new(Probe {
                count: 0,
                submissions: 0,
                drops: Arc::clone(&drops),
                panic_tick,
                fail_tick,
            }),
            drops,
        )
    }

    fn handle() -> *mut ApplicationHandle {
        Box::into_raw(Box::new(ApplicationHandle::new(probe(false, false).0)))
    }

    type Reader = unsafe fn(*mut ApplicationHandle, *mut u8, usize, *mut usize) -> i32;

    unsafe fn read(handle: *mut ApplicationHandle, reader: Reader) -> Vec<u8> {
        let mut required = usize::MAX;
        let status = reader(handle, ptr::null_mut(), 0, &mut required);
        assert!(status == BUFFER_TOO_SMALL || (status == OK && required == 0));
        let mut bytes = vec![0; required];
        assert_eq!(
            reader(handle, bytes.as_mut_ptr(), bytes.len(), &mut required),
            OK
        );
        bytes
    }

    fn input(address: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "metadata":{"source":"test", "messageId":1, "schemaVersion":1},
            "bundle":{"messages":[{"address":address, "args":[]}]}
        }))
        .unwrap()
    }

    #[test]
    fn host_inspection_is_optional_and_separate_from_pending_game_output() {
        struct Hosted(Probe);
        impl ApplicationDriver for Hosted {
            fn submit(
                &mut self,
                bundle: OscBundle,
                metadata: Option<InputMetadata>,
            ) -> Result<u64, String> {
                self.0.submit(bundle, metadata)
            }
            fn tick(&mut self) -> Result<Vec<OscBundle>, String> {
                self.0.tick()
            }
            fn inspect(&self) -> Result<Vec<OscBundle>, String> {
                self.0.inspect()
            }
            fn inspect_host(&self) -> Result<Vec<OscBundle>, String> {
                let mut bundle = OscBundle::new();
                let mut message = OscMessage::new("/host/test/session");
                message.push_arg(OscArg::Str("independent-session".into()));
                bundle.push(message);
                Ok(vec![bundle])
            }
        }
        unsafe {
            let unhosted = handle();
            assert_eq!(read(unhosted, inspect_host_json), b"[]");
            assert_eq!(destroy(unhosted), OK);
            let hosted = Box::into_raw(Box::new(ApplicationHandle::new(Box::new(Hosted(Probe {
                count: 0,
                submissions: 0,
                drops: Arc::new(AtomicUsize::new(0)),
                panic_tick: false,
                fail_tick: false,
            })))));
            let initial_host = read(hosted, inspect_host_json);
            assert!(String::from_utf8_lossy(&initial_host).contains("/host/test/session"));
            assert_eq!(tick(hosted), OK);
            let game = read(hosted, inspect_json);
            let mut required = 0;
            let mut short = [0xa5];
            assert_eq!(
                inspect_host_json(hosted, short.as_mut_ptr(), 1, &mut required),
                BUFFER_TOO_SMALL
            );
            assert_eq!(short, [0xa5]);
            assert_eq!(
                inspect_host_json(hosted, ptr::null_mut(), 1, &mut required),
                INVALID_ARGUMENT
            );
            assert_eq!(read(hosted, inspect_host_json), initial_host);
            assert_eq!(read(hosted, inspect_json), game);
            assert_eq!(tick(hosted), PENDING_OUTPUT);
            assert_eq!(read(hosted, read_output), game);
            assert_eq!(destroy(hosted), OK);
        }
    }

    #[test]
    fn buffer_queries_keep_the_entire_batch_and_never_advance() {
        unsafe {
            let handle = handle();
            let initial = read(handle, inspect_json);
            assert_eq!(tick(handle), OK);
            let after_tick = read(handle, inspect_json);
            assert_ne!(initial, after_tick);
            assert_eq!(tick(handle), PENDING_OUTPUT);
            assert_eq!(read(handle, inspect_json), after_tick);
            let mut required = 0;
            assert_eq!(
                read_output(handle, ptr::null_mut(), 1, &mut required),
                INVALID_ARGUMENT
            );
            assert_eq!(
                read_output(handle, ptr::null_mut(), 0, ptr::null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(
                read_output(handle, ptr::null_mut(), 0, &mut required),
                BUFFER_TOO_SMALL
            );
            let mut short = vec![0xa5; required - 1];
            assert_eq!(
                read_output(handle, short.as_mut_ptr(), short.len(), &mut required),
                BUFFER_TOO_SMALL
            );
            assert!(short.iter().all(|byte| *byte == 0xa5));
            assert_eq!(read(handle, read_output), after_tick);
            assert_eq!(
                read_output(handle, ptr::null_mut(), 0, &mut required),
                EMPTY
            );
            assert_eq!(required, 0);
            assert_eq!(tick(handle), OK);
            read(handle, read_output);
            assert_eq!(destroy(handle), OK);
        }
    }

    #[test]
    fn malformed_requests_and_output_pointers_never_admit_an_input() {
        unsafe {
            let handle = handle();
            let initial = read(handle, inspect_json);
            let valid = input("/test/input");
            let mut sequence = 99;
            assert_eq!(
                submit_json(handle, valid.as_ptr(), valid.len(), ptr::null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(
                submit_json(handle, ptr::null(), 1, &mut sequence),
                INVALID_ARGUMENT
            );
            assert_eq!(
                submit_json(handle, ptr::null(), MAX_INPUT_BYTES + 1, &mut sequence),
                INVALID_INPUT
            );
            let mut invalid = vec![vec![0xff], b"{}".to_vec(), b"null".to_vec(), Vec::new()];
            for (key, value) in [
                ("source", serde_json::json!("")),
                ("source", serde_json::json!("x".repeat(129))),
                ("source", serde_json::json!("a\0b")),
                ("messageId", serde_json::json!(0)),
                ("schemaVersion", serde_json::json!(0)),
            ] {
                let mut request: serde_json::Value = serde_json::from_slice(&valid).unwrap();
                request["metadata"][key] = value;
                invalid.push(serde_json::to_vec(&request).unwrap());
            }
            let mut unknown: serde_json::Value = serde_json::from_slice(&valid).unwrap();
            unknown["extra"] = serde_json::json!(true);
            invalid.push(serde_json::to_vec(&unknown).unwrap());
            for bytes in invalid {
                assert_eq!(
                    submit_json(handle, bytes.as_ptr(), bytes.len(), &mut sequence),
                    INVALID_INPUT
                );
                assert_eq!(sequence, 99);
            }
            let mut required = 99;
            assert_eq!(
                inspect_json(handle, ptr::null_mut(), 1, &mut required),
                INVALID_ARGUMENT
            );
            assert_eq!(
                inspect_json(handle, ptr::null_mut(), 0, ptr::null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(read(handle, inspect_json), initial);
            assert_eq!(
                submit_json(handle, valid.as_ptr(), valid.len(), &mut sequence),
                OK
            );
            assert_eq!(sequence, 0);
            let rejected = input("/reject");
            sequence = 99;
            assert_eq!(
                submit_json(handle, rejected.as_ptr(), rejected.len(), &mut sequence),
                DRIVER_ERROR
            );
            assert_eq!(sequence, 99);
            assert_eq!(
                submit_json(handle, valid.as_ptr(), valid.len(), &mut sequence),
                OK
            );
            assert_eq!(sequence, 1);
            assert_eq!(destroy(handle), OK);
        }
    }

    #[test]
    fn admission_budgets_reset_only_after_a_successful_tick() {
        unsafe {
            let handle = handle();
            let bytes = input("/test/input");
            let mut sequence = 0;
            for expected in 0..MAX_PENDING_INPUTS {
                assert_eq!(
                    submit_json(handle, bytes.as_ptr(), bytes.len(), &mut sequence),
                    OK
                );
                assert_eq!(sequence, expected as u64);
            }
            assert_eq!(
                submit_json(handle, bytes.as_ptr(), bytes.len(), &mut sequence),
                QUEUE_FULL
            );
            assert_eq!(tick(handle), OK);
            read(handle, read_output);
            // Whitespace counts toward the serialized byte budget too.
            let mut large = bytes;
            large.resize(MAX_INPUT_BYTES, b' ');
            for _ in 0..MAX_PENDING_INPUT_BYTES / MAX_INPUT_BYTES {
                assert_eq!(
                    submit_json(handle, large.as_ptr(), large.len(), &mut sequence),
                    OK
                );
            }
            assert_eq!(
                submit_json(handle, large.as_ptr(), large.len(), &mut sequence),
                QUEUE_FULL
            );
            assert_eq!(destroy(handle), OK);
        }
    }

    #[test]
    fn creation_negotiates_abi_and_preserves_failure_diagnostics_without_handles() {
        unsafe {
            let mut output = ptr::dangling_mut();
            let mut required = 0;
            assert_eq!(
                create_json(
                    ABI_VERSION + 1,
                    ptr::null(),
                    0,
                    &mut output,
                    ptr::null_mut(),
                    0,
                    &mut required,
                    |_| panic!("must not call factory")
                ),
                ABI_MISMATCH
            );
            assert!(output.is_null());
            assert!(required > 0);
            let mut error = vec![0xa5; 3];
            assert_eq!(
                create_json(
                    ABI_VERSION,
                    ptr::null(),
                    0,
                    &mut output,
                    error.as_mut_ptr(),
                    error.len(),
                    &mut required,
                    |_| Err("invalid application configuration".to_string())
                ),
                DRIVER_ERROR
            );
            assert!(output.is_null());
            assert_eq!(error, vec![0xa5; 3]);
            error.resize(required, 0);
            assert_eq!(
                create_json(
                    ABI_VERSION,
                    ptr::null(),
                    0,
                    &mut output,
                    error.as_mut_ptr(),
                    error.len(),
                    &mut required,
                    |_| Err("invalid application configuration".to_string())
                ),
                DRIVER_ERROR
            );
            assert_eq!(
                String::from_utf8(error).unwrap(),
                "invalid application configuration"
            );
            let (driver, drops) = probe(false, false);
            assert_eq!(
                create_json(
                    ABI_VERSION,
                    ptr::null(),
                    0,
                    &mut output,
                    ptr::null_mut(),
                    0,
                    &mut required,
                    |bytes| {
                        assert!(bytes.is_empty());
                        Ok(driver)
                    }
                ),
                OK
            );
            assert!(!output.is_null());
            assert_eq!(required, 0);
            assert_eq!(destroy(output), OK);
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn creation_validates_required_pointers_before_invoking_factory() {
        unsafe {
            let mut output = ptr::null_mut();
            let mut required = 0;
            assert_eq!(
                create_json(
                    ABI_VERSION,
                    ptr::null(),
                    0,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    0,
                    &mut required,
                    |_| panic!("unreachable")
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                create_json(
                    ABI_VERSION,
                    ptr::null(),
                    0,
                    &mut output,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                    |_| panic!("unreachable")
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                create_json(
                    ABI_VERSION,
                    ptr::null(),
                    0,
                    &mut output,
                    ptr::null_mut(),
                    1,
                    &mut required,
                    |_| panic!("unreachable")
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                create_json(
                    ABI_VERSION,
                    ptr::null(),
                    1,
                    &mut output,
                    ptr::null_mut(),
                    0,
                    &mut required,
                    |_| panic!("unreachable")
                ),
                INVALID_ARGUMENT
            );
            assert!(output.is_null());
            assert_eq!(tick(ptr::null_mut()), INVALID_ARGUMENT);
            assert_eq!(destroy(ptr::null_mut()), INVALID_ARGUMENT);
        }
    }

    #[test]
    fn wrong_thread_calls_cannot_change_or_destroy_the_handle() {
        unsafe {
            let (driver, drops) = probe(false, false);
            let handle = Box::into_raw(Box::new(ApplicationHandle::new(driver)));
            let before = read(handle, inspect_json);
            let address = handle as usize;
            thread::spawn(move || {
                let handle = address as *mut ApplicationHandle;
                assert_eq!(tick(handle), WRONG_THREAD);
                assert_eq!(destroy(handle), WRONG_THREAD);
                let mut required = 0;
                assert_eq!(
                    last_error(handle, ptr::null_mut(), 0, &mut required),
                    WRONG_THREAD
                );
            })
            .join()
            .unwrap();
            assert_eq!(drops.load(Ordering::SeqCst), 0);
            assert_eq!(read(handle, inspect_json), before);
            assert_eq!(destroy(handle), OK);
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn panic_and_failed_ticks_poison_the_handle_but_allow_diagnosis_and_destruction() {
        unsafe {
            for (panic_tick, fail_tick, expected) in
                [(true, false, PANIC), (false, true, DRIVER_ERROR)]
            {
                let (driver, drops) = probe(panic_tick, fail_tick);
                let handle = Box::into_raw(Box::new(ApplicationHandle::new(driver)));
                assert_eq!(tick(handle), expected);
                assert_eq!(tick(handle), FAILED);
                let diagnostic = read(handle, last_error);
                assert!(!diagnostic.is_empty());
                assert_eq!(read(handle, last_error), diagnostic);
                let mut required = 0;
                assert_eq!(
                    inspect_json(handle, ptr::null_mut(), 0, &mut required),
                    FAILED
                );
                assert_eq!(destroy(handle), OK);
                assert_eq!(drops.load(Ordering::SeqCst), 1);
            }
            let mut output = ptr::null_mut();
            let mut required = 0;
            assert_eq!(
                create_json(
                    ABI_VERSION,
                    ptr::null(),
                    0,
                    &mut output,
                    ptr::null_mut(),
                    0,
                    &mut required,
                    |_| panic!("factory panicked")
                ),
                PANIC
            );
            assert!(output.is_null());
            assert!(required > 0);
        }
    }

    #[test]
    fn runtime_adapter_uses_the_existing_queue_and_tick_order() {
        let mut direct = build_runtime(LocalChannel::connected());
        let runtime = build_runtime(LocalChannel::connected());
        let handle = Box::into_raw(Box::new(ApplicationHandle::new(Box::new(
            RuntimeDriver::new(runtime),
        ))));
        let request: InputRequest = serde_json::from_value(serde_json::json!({
            "bundle":{"messages":[{"address":"/input/move", "args":[
                {"type":"str","value":"player"},
                {"type":"float","value":0.5},
                {"type":"float","value":-0.75}
            ]}]}
        }))
        .unwrap();
        let expected_sequence = direct
            .try_enqueue_input(OscBundle::try_from(request.bundle.clone()).unwrap(), None)
            .unwrap();
        let bytes = serde_json::to_vec(&request).unwrap();
        unsafe {
            let mut sequence = u64::MAX;
            assert_eq!(
                submit_json(handle, bytes.as_ptr(), bytes.len(), &mut sequence),
                OK
            );
            assert_eq!(sequence, expected_sequence);
            assert_eq!(read(handle, inspect_json), b"[]");
            assert_eq!(tick(handle), OK);
            direct.tick_once().unwrap();
            assert_eq!(
                read(handle, read_output),
                encode_bundles(direct.drain_output_buffer()).unwrap()
            );
            assert_eq!(tick(handle), OK);
            direct.tick_once().unwrap();
            assert_eq!(
                read(handle, read_output),
                encode_bundles(direct.drain_output_buffer()).unwrap()
            );
            assert_eq!(destroy(handle), OK);
        }
    }

    #[test]
    fn factory_capture_destructors_and_panic_payloads_cannot_unwind_through_the_boundary() {
        struct PanicOnDrop;
        impl Drop for PanicOnDrop {
            fn drop(&mut self) {
                panic!("captured value or panic payload dropped");
            }
        }

        unsafe {
            let mut output = ptr::dangling_mut();
            let capture = PanicOnDrop;
            // Invalid pointers prevent invoking the factory, but its owned
            // capture still drops inside the catch boundary.
            assert_eq!(
                create_json(
                    ABI_VERSION,
                    ptr::null(),
                    0,
                    &mut output,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                    move |_| {
                        drop(capture);
                        unreachable!("invalid output arguments must prevent factory invocation")
                    }
                ),
                PANIC
            );
            assert!(output.is_null());
            let mut required = 0;
            assert_eq!(
                create_json(
                    ABI_VERSION,
                    ptr::null(),
                    0,
                    &mut output,
                    ptr::null_mut(),
                    0,
                    &mut required,
                    |_| {
                        std::panic::panic_any(PanicOnDrop);
                    }
                ),
                PANIC
            );
            assert!(output.is_null());
            assert!(required > 0);
        }
    }

    fn string_batch(length: usize) -> Vec<OscBundle> {
        vec![OscBundle {
            messages: vec![OscMessage {
                address: "/test/large".into(),
                args: vec![OscArg::Str("x".repeat(length))],
            }],
        }]
    }

    #[test]
    fn output_limit_uses_exact_encoded_bytes_and_stops_before_later_values() {
        let overhead = encode_bundles(string_batch(0)).unwrap().len();
        assert_eq!(
            encode_bundles(string_batch(MAX_OUTPUT_BYTES - overhead))
                .unwrap()
                .len(),
            MAX_OUTPUT_BYTES,
            "an exactly fitting batch must not be rejected by a size estimate"
        );
        let mut bundles = string_batch(MAX_OUTPUT_BYTES + 1);
        bundles.push(OscBundle {
            messages: vec![OscMessage::new("invalid")],
        });
        let error = encode_bundles(bundles).unwrap_err();
        assert!(
            error.contains("output exceeds 64 MiB ABI limit"),
            "the writer must stop on the first oversized string before any \
             conversion of later bundles: {error}"
        );

        // A normal doubling from half the maximum plus one would reserve more
        // than the limit when the remaining, still valid bytes are written.
        let bytes = vec![b'x'; MAX_OUTPUT_BYTES];
        let mut writer = LimitedBuffer(Vec::new());
        let split = MAX_OUTPUT_BYTES / 2 + 1;
        writer.write_all(&bytes[..split]).unwrap();
        writer.write_all(&bytes[split..]).unwrap();
        assert_eq!(writer.0.len(), MAX_OUTPUT_BYTES);
        assert!(writer.0.capacity() <= MAX_OUTPUT_BYTES);
        assert!(writer.write_all(b"x").is_err());
        assert_eq!(writer.0.len(), MAX_OUTPUT_BYTES);
    }

    #[test]
    fn oversized_inspection_is_recoverable_but_a_tick_batch_failure_poisons() {
        struct LargeOutput {
            ticks: Arc<AtomicUsize>,
        }
        impl ApplicationDriver for LargeOutput {
            fn submit(&mut self, _: OscBundle, _: Option<InputMetadata>) -> Result<u64, String> {
                Ok(0)
            }
            fn tick(&mut self) -> Result<Vec<OscBundle>, String> {
                self.ticks.fetch_add(1, Ordering::SeqCst);
                Ok(string_batch(MAX_OUTPUT_BYTES + 1))
            }
            fn inspect(&self) -> Result<Vec<OscBundle>, String> {
                Ok(string_batch(MAX_OUTPUT_BYTES + 1))
            }
        }
        unsafe {
            let ticks = Arc::new(AtomicUsize::new(0));
            let handle = Box::into_raw(Box::new(ApplicationHandle::new(Box::new(LargeOutput {
                ticks: Arc::clone(&ticks),
            }))));
            let mut required = 99;
            assert_eq!(
                inspect_json(handle, ptr::null_mut(), 0, &mut required),
                DRIVER_ERROR
            );
            assert_eq!(ticks.load(Ordering::SeqCst), 0);
            assert!(String::from_utf8(read(handle, last_error))
                .unwrap()
                .contains("output exceeds 64 MiB ABI limit"));
            assert_eq!(tick(handle), DRIVER_ERROR);
            assert_eq!(ticks.load(Ordering::SeqCst), 1);
            assert_eq!(tick(handle), FAILED);
            assert_eq!(ticks.load(Ordering::SeqCst), 1);
            assert_eq!(
                read_output(handle, ptr::null_mut(), 0, &mut required),
                FAILED,
                "no partial serialization may escape from a failed tick"
            );
            assert!(String::from_utf8(read(handle, last_error))
                .unwrap()
                .contains("output exceeds 64 MiB ABI limit"));
            assert_eq!(destroy(handle), OK);
        }
    }
}

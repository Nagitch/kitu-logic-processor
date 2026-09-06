#ifndef KITU_APPLICATION_H
#define KITU_APPLICATION_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque, application-owned allocation. Never dereference or free() this pointer. */
typedef struct KituApplication KituApplication;

#define KITU_APPLICATION_ABI_VERSION UINT32_C(1)
#define KITU_APPLICATION_OK INT32_C(0)
#define KITU_APPLICATION_EMPTY INT32_C(1)
#define KITU_APPLICATION_BUFFER_TOO_SMALL INT32_C(2)
#define KITU_APPLICATION_INVALID_ARGUMENT (-INT32_C(1))
#define KITU_APPLICATION_INVALID_INPUT (-INT32_C(2))
#define KITU_APPLICATION_WRONG_THREAD (-INT32_C(3))
#define KITU_APPLICATION_PENDING_OUTPUT (-INT32_C(4))
#define KITU_APPLICATION_FAILED (-INT32_C(5))
#define KITU_APPLICATION_PANIC (-INT32_C(6))
#define KITU_APPLICATION_QUEUE_FULL (-INT32_C(7))
#define KITU_APPLICATION_DRIVER_ERROR (-INT32_C(8))
#define KITU_APPLICATION_ABI_MISMATCH (-INT32_C(9))

#define KITU_APPLICATION_MAX_INPUT_BYTES ((size_t)1048576)
#define KITU_APPLICATION_MAX_PENDING_INPUTS ((size_t)4096)
#define KITU_APPLICATION_MAX_PENDING_INPUT_BYTES ((size_t)16777216)
#define KITU_APPLICATION_MAX_OUTPUT_BYTES ((size_t)67108864)

/*
 * Every operation on a handle, including destroy, must be sequential on the
 * creating thread. Callers must not concurrently access a handle or its buffers.
 * Pointers must be valid/aligned for their complete lengths, and must not alias
 * the handle or another input/output argument. Dangling pointers, double destroy,
 * and invalid memory remain caller errors; this API cannot validate addresses.
 *
 * All byte buffers belong to the caller and are borrowed only during a call.
 * Text is UTF-8 with an explicit byte length, never a trailing NUL requirement.
 * NULL byte buffers are valid only with zero length/capacity. Required-length
 * pointers are mandatory. A short buffer receives no partial bytes and the
 * exact required length is returned. Input JSON/configuration is limited to
 * MAX_INPUT_BYTES; configuration schema belongs to the application factory.
 */
uint32_t kitu_application_abi_version(void);

/*
 * Creates exclusively owned application state. NULL config with length zero
 * requests the application's defaults, when supported by its factory.
 * On failure, *out_handle is NULL and no handle ownership is transferred.
 * Creation errors are written only if the entire diagnostic fits error_capacity;
 * *out_error_required always reports its complete size. The original error
 * status is preserved for a short diagnostic buffer. Repeat only FAILED creation
 * to retrieve a longer diagnostic; repeating success would create a second run.
 * The application library must be built with panic unwinding to catch Rust panics.
 */
int32_t kitu_application_create(
    uint32_t requested_abi,
    const uint8_t *config,
    size_t config_length,
    KituApplication **out_handle,
    uint8_t *error_buffer,
    size_t error_capacity,
    size_t *out_error_required);

/* OK or PANIC consumes ownership. Other statuses leave ownership with the caller. */
int32_t kitu_application_destroy(KituApplication *handle);

/*
 * Queues one InputRequest JSON: {"metadata":optional_metadata,"bundle":bundle}.
 * Metadata: {"source":"producer","messageId":1,"schemaVersion":1}.
 * Bundle: {"messages":[{"address":"/input/...","args":[{"type":"int","value":1}]}]}.
 * Argument types: int (i32), int64 (i64), float (finite f32), str (UTF-8), bool.
 * Unknown JSON fields are rejected. OSC messages/args retain their order.
 * Metadata requires source 1..128 UTF-8 bytes without NUL and positive IDs/version.
 * Application validation determines whether metadata is required and which schema
 * and addresses it supports. OK writes admission order to *out_sequence; an error
 * leaves it unchanged. Admission does not mean application acceptance: consume
 * the later tick's command receipt for success/refusal at the authoritative tick.
 */
int32_t kitu_application_submit_json(
    KituApplication *handle,
    const uint8_t *input,
    size_t input_length,
    uint64_t *out_sequence);

/*
 * Advances exactly one management tick. The caller schedules the agreed tick rate
 * independently of input submission and rendering; the library starts no timer.
 * PENDING_OUTPUT refuses before advancing. Even [] is a batch requiring a read.
 * A driver/encoding error or panic during ticking permanently disables the handle.
 * After failure only last_error and destroy are permitted.
 */
int32_t kitu_application_tick(KituApplication *handle);

/*
 * Reads one complete JSON array of typed OSC bundles. OK consumes the whole batch.
 * BUFFER_TOO_SMALL/invalid arguments retain it. NULL buffer/capacity 0 queries
 * size. EMPTY reports required length 0 after consumption or before the first tick.
 */
int32_t kitu_application_read_output(
    KituApplication *handle,
    uint8_t *buffer,
    size_t capacity,
    size_t *out_required);

/* Nonmutating projection, same JSON array shape; no output is consumed. */
int32_t kitu_application_inspect_json(
    KituApplication *handle,
    uint8_t *buffer,
    size_t capacity,
    size_t *out_required);

/* Persistent last UTF-8 diagnostic, including on a failed handle. Initially empty. */
int32_t kitu_application_last_error(
    KituApplication *handle,
    uint8_t *buffer,
    size_t capacity,
    size_t *out_required);

#ifdef __cplusplus
}
#endif

#endif /* KITU_APPLICATION_H */

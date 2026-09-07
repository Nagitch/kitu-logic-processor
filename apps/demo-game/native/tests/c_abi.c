/* Actual C caller for the application-owned native library.
 *
 * Usage: c_abi TRACE_PATH ACTUAL_NDJSON_PATH
 * Trace records are I<JSON InputRequest> or T, one record per line. The Rust
 * fixture generator supplies expected output from an ordinary Runtime. This
 * caller writes the native library's unmodified JSON for an external comparison;
 * it does not implement a second game simulation or a JSON parser.
 */
#define _POSIX_C_SOURCE 200809L

#include "kitu_application.h"

#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>

#define MAX_BATCH_BYTES ((size_t)64 * 1024 * 1024)

typedef int32_t (*Reader)(KituApplication *, uint8_t *, size_t, size_t *);

static KituApplication *application;
static FILE *trace_file;
static FILE *actual_file;
static char *trace_line;
static size_t trace_line_number;

static void cleanup(void) {
    if (application != NULL) {
        (void)kitu_application_destroy(application);
        application = NULL;
    }
    if (trace_file != NULL) {
        (void)fclose(trace_file);
        trace_file = NULL;
    }
    if (actual_file != NULL) {
        (void)fclose(actual_file);
        actual_file = NULL;
    }
    free(trace_line);
    trace_line = NULL;
}

static void fail(const char *message, int source_line) {
    fprintf(stderr, "C ABI failure at source line %d, trace line %zu: %s\n",
            source_line, trace_line_number, message);
    exit(EXIT_FAILURE);
}

#define REQUIRE(condition, message)                                             \
    do {                                                                        \
        if (!(condition)) {                                                     \
            fail((message), __LINE__);                                          \
        }                                                                       \
    } while (0)

static void expect_status(int32_t actual, int32_t expected,
                          const char *operation, int source_line) {
    if (actual != expected) {
        fprintf(stderr,
                "C ABI status at source line %d, trace line %zu: %s returned "
                "%" PRId32 ", expected %" PRId32 "\n",
                source_line, trace_line_number, operation, actual, expected);
        exit(EXIT_FAILURE);
    }
}

#define EXPECT(operation, status)                                               \
    expect_status((operation), (status), #operation, __LINE__)

static uint8_t *allocate_bytes(size_t length) {
    REQUIRE(length <= MAX_BATCH_BYTES, "batch exceeds documented 64 MiB limit");
    uint8_t *bytes = malloc(length + 1);
    REQUIRE(bytes != NULL, "cannot allocate caller-owned buffer");
    return bytes;
}

/* Verify that a query and a deliberately short buffer leave the complete batch
 * available. Sentinels detect partial writes even when the function returns the
 * expected error. The final successful call uses the exact required capacity.
 */
static char *read_checked(Reader reader, size_t *out_length) {
    size_t required = SIZE_MAX;
    EXPECT(reader(application, NULL, 0, &required),
           KITU_APPLICATION_BUFFER_TOO_SMALL);
    REQUIRE(required > 0, "nonempty batch must report a positive byte count");
    uint8_t *bytes = allocate_bytes(required);
    memset(bytes, 0xa5, required);

    size_t short_required = SIZE_MAX;
    EXPECT(reader(application, bytes, required - 1, &short_required),
           KITU_APPLICATION_BUFFER_TOO_SMALL);
    REQUIRE(short_required == required, "short read changed the required size");
    for (size_t index = 0; index < required; ++index) {
        REQUIRE(bytes[index] == 0xa5, "short read partially modified the buffer");
    }

    size_t copied = SIZE_MAX;
    EXPECT(reader(application, bytes, required, &copied), KITU_APPLICATION_OK);
    REQUIRE(copied == required, "full read changed the required size");
    bytes[copied] = '\0';
    REQUIRE(memchr(bytes, '\0', copied) == NULL,
            "UTF-8 JSON or diagnostic unexpectedly contains a raw NUL byte");
    *out_length = copied;
    return (char *)bytes;
}

static char *inspect_stable(size_t *out_length) {
    size_t first_length = 0;
    char *first = read_checked(kitu_application_inspect_json, &first_length);
    size_t second_length = 0;
    char *second = read_checked(kitu_application_inspect_json, &second_length);
    REQUIRE(first_length == second_length &&
                memcmp(first, second, first_length) == 0,
            "inspection advanced or mutated the application");
    free(second);
    *out_length = first_length;
    return first;
}

static void create_default(void) {
    size_t error_required = SIZE_MAX;
    REQUIRE(application == NULL, "create would overwrite an owned handle");
    EXPECT(kitu_application_create(KITU_APPLICATION_ABI_VERSION, NULL, 0,
                                   &application, NULL, 0, &error_required),
           KITU_APPLICATION_OK);
    REQUIRE(application != NULL, "successful creation returned a null handle");
    REQUIRE(error_required == 0, "successful creation returned an error");
}

static void check_failed_creation(uint32_t abi, const uint8_t *config,
                                  size_t config_length, int32_t expected) {
    KituApplication *rejected = NULL;
    size_t required = SIZE_MAX;
    EXPECT(kitu_application_create(abi, config, config_length, &rejected, NULL,
                                   0, &required),
           expected);
    REQUIRE(rejected == NULL, "failed creation returned an owned handle");
    REQUIRE(required > 0, "failed creation did not provide a diagnostic size");
    uint8_t *diagnostic = allocate_bytes(required);
    size_t copied = SIZE_MAX;
    EXPECT(kitu_application_create(abi, config, config_length, &rejected,
                                   diagnostic, required, &copied),
           expected);
    REQUIRE(rejected == NULL, "repeated failed creation returned a handle");
    REQUIRE(copied == required, "creation diagnostic changed between queries");
    REQUIRE(memchr(diagnostic, '\0', copied) == NULL,
            "creation diagnostic includes an undocumented NUL terminator");
    free(diagnostic);
}

static void check_creation_and_invalid_arguments(void) {
    REQUIRE(kitu_application_abi_version() == 1,
            "native library reports an unexpected ABI version");
    REQUIRE(KITU_APPLICATION_ABI_VERSION == 1,
            "header and test disagree on the ABI version");
    check_failed_creation(999, NULL, 0, KITU_APPLICATION_ABI_MISMATCH);
    const char bad_contract[] = "{\"contractVersion\":999}";
    check_failed_creation(KITU_APPLICATION_ABI_VERSION,
                          (const uint8_t *)bad_contract,
                          sizeof(bad_contract) - 1,
                          KITU_APPLICATION_DRIVER_ERROR);

    size_t required = SIZE_MAX;
    KituApplication *rejected = NULL;
    EXPECT(kitu_application_create(KITU_APPLICATION_ABI_VERSION, NULL, 0, NULL,
                                   NULL, 0, &required),
           KITU_APPLICATION_INVALID_ARGUMENT);
    EXPECT(kitu_application_create(KITU_APPLICATION_ABI_VERSION, NULL, 1,
                                   &rejected, NULL, 0, &required),
           KITU_APPLICATION_INVALID_ARGUMENT);
    REQUIRE(rejected == NULL, "invalid creation arguments returned a handle");
    EXPECT(kitu_application_create(KITU_APPLICATION_ABI_VERSION, NULL, 0,
                                   &rejected, NULL, 0, NULL),
           KITU_APPLICATION_INVALID_ARGUMENT);
    REQUIRE(rejected == NULL, "missing error length pointer returned a handle");

    EXPECT(kitu_application_destroy(NULL), KITU_APPLICATION_INVALID_ARGUMENT);
    EXPECT(kitu_application_tick(NULL), KITU_APPLICATION_INVALID_ARGUMENT);
    EXPECT(kitu_application_read_output(NULL, NULL, 0, &required),
           KITU_APPLICATION_INVALID_ARGUMENT);
    EXPECT(kitu_application_inspect_json(NULL, NULL, 0, &required),
           KITU_APPLICATION_INVALID_ARGUMENT);
    EXPECT(kitu_application_last_error(NULL, NULL, 0, &required),
           KITU_APPLICATION_INVALID_ARGUMENT);

    create_default();
    EXPECT(kitu_application_last_error(application, NULL, 0, &required),
           KITU_APPLICATION_OK);
    REQUIRE(required == 0, "new handle unexpectedly has a diagnostic");
    size_t initial_length = 0;
    char *initial = inspect_stable(&initial_length);

    uint64_t sequence = UINT64_MAX;
    const uint8_t invalid_utf8[] = {0xff};
    EXPECT(kitu_application_submit_json(application, invalid_utf8,
                                        sizeof(invalid_utf8), &sequence),
           KITU_APPLICATION_INVALID_INPUT);
    REQUIRE(sequence == UINT64_MAX, "invalid UTF-8 modified output sequence");
    size_t diagnostic_length = 0;
    char *diagnostic = read_checked(kitu_application_last_error,
                                    &diagnostic_length);
    REQUIRE(diagnostic_length > 0, "invalid UTF-8 has no diagnostic");
    free(diagnostic);

    EXPECT(kitu_application_submit_json(NULL, invalid_utf8,
                                        sizeof(invalid_utf8), &sequence),
           KITU_APPLICATION_INVALID_ARGUMENT);
    EXPECT(kitu_application_submit_json(application, NULL, 1, &sequence),
           KITU_APPLICATION_INVALID_ARGUMENT);
    const char empty_bundle[] = "{\"bundle\":{\"messages\":[]}}";
    EXPECT(kitu_application_submit_json(application,
                                        (const uint8_t *)empty_bundle,
                                        sizeof(empty_bundle) - 1, NULL),
           KITU_APPLICATION_INVALID_ARGUMENT);
    REQUIRE(sequence == UINT64_MAX, "invalid arguments modified sequence");
    EXPECT(kitu_application_read_output(application, NULL, 0, NULL),
           KITU_APPLICATION_INVALID_ARGUMENT);
    EXPECT(kitu_application_read_output(application, NULL, 1, &required),
           KITU_APPLICATION_INVALID_ARGUMENT);
    EXPECT(kitu_application_inspect_json(application, NULL, 0, NULL),
           KITU_APPLICATION_INVALID_ARGUMENT);
    EXPECT(kitu_application_inspect_json(application, NULL, 1, &required),
           KITU_APPLICATION_INVALID_ARGUMENT);
    EXPECT(kitu_application_last_error(application, NULL, 0, NULL),
           KITU_APPLICATION_INVALID_ARGUMENT);
    EXPECT(kitu_application_last_error(application, NULL, 1, &required),
           KITU_APPLICATION_INVALID_ARGUMENT);

    size_t after_length = 0;
    char *after = inspect_stable(&after_length);
    REQUIRE(initial_length == after_length &&
                memcmp(initial, after, initial_length) == 0,
            "rejected arguments or inspection changed the initial state");
    free(initial);
    free(after);
    EXPECT(kitu_application_read_output(application, NULL, 0, &required),
           KITU_APPLICATION_EMPTY);
    REQUIRE(required == 0, "rejected requests created tick output");
}

static char *tick_and_drain(size_t *output_length) {
    EXPECT(kitu_application_tick(application), KITU_APPLICATION_OK);
    EXPECT(kitu_application_tick(application), KITU_APPLICATION_PENDING_OUTPUT);
    EXPECT(kitu_application_read_output(application, NULL, 0, NULL),
           KITU_APPLICATION_INVALID_ARGUMENT);
    char *output = read_checked(kitu_application_read_output, output_length);
    size_t required = SIZE_MAX;
    EXPECT(kitu_application_read_output(application, NULL, 0, &required),
           KITU_APPLICATION_EMPTY);
    REQUIRE(required == 0, "consumed output remained available");
    return output;
}

static void destroy_owned(void) {
    KituApplication *owned = application;
    application = NULL;
    EXPECT(kitu_application_destroy(owned), KITU_APPLICATION_OK);
}

int main(int argc, char **argv) {
    REQUIRE(atexit(cleanup) == 0, "cannot install cleanup handler");
    REQUIRE(argc == 3, "usage: c_abi TRACE_PATH ACTUAL_NDJSON_PATH");
    trace_file = fopen(argv[1], "r");
    REQUIRE(trace_file != NULL, "cannot open trace input");
    actual_file = fopen(argv[2], "w");
    REQUIRE(actual_file != NULL, "cannot open actual NDJSON output");
    check_creation_and_invalid_arguments();

    uint64_t inputs = 0;
    uint64_t ticks = 0;
    size_t line_capacity = 0;
    ssize_t line_length;
    while ((line_length = getline(&trace_line, &line_capacity, trace_file)) >= 0) {
        ++trace_line_number;
        size_t length = (size_t)line_length;
        if (length > 0 && trace_line[length - 1] == '\n') {
            --length;
        }
        if (length > 0 && trace_line[length - 1] == '\r') {
            --length;
        }
        REQUIRE(length > 0, "empty trace record");
        if (trace_line[0] == 'I') {
            REQUIRE(length > 1, "input record contains no JSON");
            uint64_t sequence = UINT64_MAX;
            EXPECT(kitu_application_submit_json(
                       application, (const uint8_t *)trace_line + 1, length - 1,
                       &sequence),
                   KITU_APPLICATION_OK);
            REQUIRE(sequence == inputs, "admission sequence is not monotonic");
            REQUIRE(inputs < UINT64_MAX, "input counter overflow");
            ++inputs;
        } else if (trace_line[0] == 'T' && length == 1) {
            size_t output_length = 0;
            char *output = tick_and_drain(&output_length);
            size_t state_length = 0;
            char *state = inspect_stable(&state_length);
            REQUIRE(output_length >= 2 && state_length >= 2,
                    "output or state is not a serialized JSON array");
            REQUIRE(fprintf(actual_file,
                            "{\"tick\":%" PRIu64 ",\"output\":%s,\"state\":%s}\n",
                            ticks, output, state) >= 0,
                    "cannot write native observation");
            free(output);
            free(state);
            REQUIRE(ticks < UINT64_MAX, "tick counter overflow");
            ++ticks;
        } else {
            fail("trace records must start with I or contain exactly T",
                 __LINE__);
        }
    }
    REQUIRE(!ferror(trace_file), "error while reading trace input");
    REQUIRE(ticks > 0, "trace did not exercise any tick");
    REQUIRE(fflush(actual_file) == 0, "cannot flush native observations");
    destroy_owned();

    for (unsigned cycle = 0; cycle < 10; ++cycle) {
        create_default();
        size_t length = 0;
        char *output = tick_and_drain(&length);
        free(output);
        char *state = inspect_stable(&length);
        free(state);
        destroy_owned();
    }
    REQUIRE(printf("{\"ticks\":%" PRIu64 ",\"inputs\":%" PRIu64
                   ",\"lifetimeCycles\":10}\n",
                   ticks, inputs) >= 0,
            "cannot write verification summary");
    return EXIT_SUCCESS;
}

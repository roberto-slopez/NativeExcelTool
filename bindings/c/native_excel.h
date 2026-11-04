#ifndef NATIVE_EXCEL_H
#define NATIVE_EXCEL_H
#include <stdint.h>
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif
/**
 * Length-delimited UTF-8 JSON owned by the native library, without a NUL terminator.
 * Free each returned allocation exactly once with ne_buffer_free, including errors.
 * Do not modify the returned pointer or length or use a different allocator.
 */
typedef struct NeBuffer { uint8_t *data; size_t len; } NeBuffer;

/** Returns the C ABI contract version. */
uint32_t ne_abi_version(void);
/** Releases a library-owned allocation. A null data pointer is a no-op. */
void ne_buffer_free(NeBuffer buffer);

/**
 * JSON operations return 0 on success and 1 on failure. A non-null out pointer must
 * be aligned and exclusively writable for one NeBuffer. Release any allocation
 * previously stored there before reuse. Both success and failure produce an owned
 * JSON response; a null out pointer returns failure without allocating a response.
 *
 * Input data must remain readable and unmodified for len bytes during the call.
 * UTF-8 and request contents are validated by the library. Invalid handles return
 * JSON errors. Release every job, query, and input handle with its matching function.
 * Query reads, job results, event polling, and input writes can block; invoke them
 * on an appropriate application worker thread.
 */
int32_t ne_capabilities_json(NeBuffer *out);
int32_t ne_schema_json(NeBuffer *out);
int32_t ne_validate_json(const uint8_t *data, size_t len, NeBuffer *out);
int32_t ne_export_start_json(const uint8_t *data, size_t len, NeBuffer *out);
int32_t ne_job_status_json(uint64_t id, NeBuffer *out);
int32_t ne_job_next_event_json(uint64_t id, NeBuffer *out);
int32_t ne_job_result_json(uint64_t id, NeBuffer *out);
int32_t ne_job_cancel(uint64_t id, NeBuffer *out);
int32_t ne_job_release(uint64_t id, NeBuffer *out);
int32_t ne_query_open_json(const uint8_t *data, size_t len, NeBuffer *out);
int32_t ne_query_schema_json(uint64_t id, NeBuffer *out);
int32_t ne_query_next_json(uint64_t id, NeBuffer *out);
int32_t ne_query_cancel(uint64_t id, NeBuffer *out);
int32_t ne_query_release(uint64_t id, NeBuffer *out);
int32_t ne_input_open_json(const uint8_t *data, size_t len, NeBuffer *out);
int32_t ne_input_write_batch_json(uint64_t id, const uint8_t *data, size_t len, NeBuffer *out);
int32_t ne_input_finish(uint64_t id, NeBuffer *out);
int32_t ne_input_release(uint64_t id, NeBuffer *out);
#ifdef __cplusplus
}
#endif
#endif

#ifndef NATIVE_EXCEL_H
#define NATIVE_EXCEL_H
#include <stdint.h>
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif
typedef struct NeBuffer { uint8_t *data; size_t len; } NeBuffer;
uint32_t ne_abi_version(void);
void ne_buffer_free(NeBuffer buffer);
int32_t ne_schema_json(NeBuffer *out);
int32_t ne_validate_json(const uint8_t *data, size_t len, NeBuffer *out);
int32_t ne_export_start_json(const uint8_t *data, size_t len, NeBuffer *out);
int32_t ne_job_status_json(uint64_t id, NeBuffer *out);
int32_t ne_job_result_json(uint64_t id, NeBuffer *out);
int32_t ne_job_cancel(uint64_t id, NeBuffer *out);
int32_t ne_job_release(uint64_t id, NeBuffer *out);
int32_t ne_query_open_json(const uint8_t *data, size_t len, NeBuffer *out);
int32_t ne_query_schema_json(uint64_t id, NeBuffer *out);
int32_t ne_query_next_json(uint64_t id, NeBuffer *out);
int32_t ne_query_release(uint64_t id, NeBuffer *out);
#ifdef __cplusplus
}
#endif
#endif

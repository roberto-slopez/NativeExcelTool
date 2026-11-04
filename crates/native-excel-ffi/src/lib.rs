//! C ABI for JSON requests, native handles, and owned response buffers.
//!
//! JSON operations return zero on success or one on failure. A non-null output
//! pointer receives an owned UTF-8 JSON buffer on either path; callers must free
//! it with [`ne_buffer_free`]. These buffers are length-delimited, not NUL-terminated.
use native_excel::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::runtime::Runtime;

/// Length-delimited UTF-8 JSON allocated by the native library.
///
/// Release a returned allocation exactly once with [`ne_buffer_free`].
#[repr(C)]
#[derive(Default)]
pub struct NeBuffer {
    /// Start of the allocation, or null for an empty buffer.
    pub data: *mut u8,
    /// Length of the allocation in bytes, excluding any NUL terminator.
    pub len: usize,
}
struct JobHandle {
    job: ExportJob,
    progress: tokio::sync::Mutex<tokio::sync::watch::Receiver<JobStatus>>,
}
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static JOBS: OnceLock<Mutex<BTreeMap<u64, Arc<JobHandle>>>> = OnceLock::new();
struct QueryHandle {
    session: tokio::sync::Mutex<QuerySession>,
    cancel: tokio::task::AbortHandle,
}
type Sessions = Mutex<BTreeMap<u64, Arc<QueryHandle>>>;
static QUERIES: OnceLock<Sessions> = OnceLock::new();
static INPUTS: OnceLock<Mutex<BTreeMap<u64, String>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| Runtime::new().expect("runtime initialization"))
}
fn jobs() -> &'static Mutex<BTreeMap<u64, Arc<JobHandle>>> {
    JOBS.get_or_init(Default::default)
}
fn queries() -> &'static Sessions {
    QUERIES.get_or_init(Default::default)
}
fn inputs() -> &'static Mutex<BTreeMap<u64, String>> {
    INPUTS.get_or_init(Default::default)
}
fn invalid_handle() -> ExportError {
    ExportError::new("INVALID_HANDLE", "Invalid or released native handle")
}
fn job_handle(id: u64) -> Result<Arc<JobHandle>> {
    jobs()
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(invalid_handle)
}
fn job(id: u64) -> Result<ExportJob> {
    Ok(job_handle(id)?.job.clone())
}
fn query(id: u64) -> Result<Arc<QueryHandle>> {
    queries()
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(invalid_handle)
}
fn input_id(id: u64) -> Result<String> {
    inputs()
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(invalid_handle)
}

/// Borrows validated UTF-8 from the caller's input buffer.
///
/// # Safety
///
/// A non-null `data` pointer must address `len` initialized bytes in one allocation
/// that remains readable and unmodified for the returned borrow's lifetime.
unsafe fn text<'a>(data: *const u8, len: usize) -> Result<&'a str> {
    if data.is_null() || len > 32 * 1024 * 1024 {
        return Err(ExportError::new(
            "INVALID_CONFIG",
            "Null input or input exceeds 32 MiB",
        ));
    }
    // SAFETY: the caller guarantees a valid shared borrow of the input allocation.
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(data, len) })
        .map_err(|_| ExportError::new("INVALID_CONFIG", "Expected UTF-8"))
}

/// Serializes a result or error into a library-owned response buffer.
///
/// # Safety
///
/// A non-null `out` must be aligned and valid for an exclusive write of one
/// `NeBuffer`. Release any allocation previously stored there before calling.
unsafe fn response(out: *mut NeBuffer, action: impl FnOnce() -> Result<serde_json::Value>) -> i32 {
    if out.is_null() {
        return 1;
    }
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(action)).unwrap_or_else(|_| {
            Err(ExportError::new(
                "INTERNAL_ERROR",
                "Native operation panicked",
            ))
        });
    let status = if result.is_ok() { 0 } else { 1 };
    let value = match result {
        Ok(v) => v,
        Err(e) => serde_json::to_value(e).unwrap(),
    };
    let mut bytes = value.to_string().into_bytes().into_boxed_slice();
    // SAFETY: null was rejected above; the caller guarantees exclusive writable storage.
    unsafe {
        *out = NeBuffer {
            data: bytes.as_mut_ptr(),
            len: bytes.len(),
        };
    }
    std::mem::forget(bytes);
    status
}

/// Returns the version of the C ABI contract.
#[unsafe(no_mangle)]
pub extern "C" fn ne_abi_version() -> u32 {
    1
}

/// Releases a buffer allocated by this library; a null data pointer is a no-op.
///
/// # Safety
///
/// A non-null buffer must retain the original pointer and length returned by this
/// library. Its allocation must not have been freed, and no references may be used
/// after this call. Do not free the allocation with a foreign allocator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ne_buffer_free(buffer: NeBuffer) {
    if !buffer.data.is_null() {
        // SAFETY: the caller supplies the original, live boxed-slice pointer and length.
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(buffer.data, buffer.len)) });
    }
}

/// Writes the supported export capabilities as an owned JSON response.
///
/// # Safety
///
/// A non-null `out` must be aligned and exclusively writable for one `NeBuffer`.
/// Release any allocation previously stored there. Free the returned buffer with
/// [`ne_buffer_free`] on both success and failure. Null returns failure without a buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ne_capabilities_json(out: *mut NeBuffer) -> i32 {
    // SAFETY: the caller upholds the output pointer and ownership requirements.
    unsafe {
        response(out, || {
            Ok(serde_json::to_value(ExcelExporter::capabilities()).unwrap())
        })
    }
}

macro_rules! json_input {
    ($name:ident, $summary:literal, $handler:expr) => {
        #[doc = $summary]
        ///
        /// # Safety
        ///
        /// A non-null `data` must address `len` initialized bytes in one allocation,
        /// readable and unmodified for this call. UTF-8 is validated at runtime.
        /// A non-null `out` must be aligned and exclusively writable for one `NeBuffer`.
        /// Release any allocation previously stored there. Free the returned buffer
        /// with `ne_buffer_free` on both success and failure. Null output returns
        /// failure without a buffer.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(data: *const u8, len: usize, out: *mut NeBuffer) -> i32 {
            // SAFETY: the caller upholds the documented input and output pointer contracts.
            unsafe {
                response(out, || {
                    let json = text(data, len)?;
                    ($handler)(json)
                })
            }
        }
    };
}

json_input!(
    ne_validate_json,
    "Validates an export request without starting an export.",
    |json: &str| {
        ExcelExporter::validate(&parse_request(json)?)?;
        Ok(serde_json::json!(true))
    }
);

json_input!(
    ne_export_start_json,
    "Starts an export and returns its native job handle as JSON.",
    |json: &str| {
        let request = parse_request(json)?;
        let _guard = runtime().enter();
        let j = ExcelExporter::start(request)?;
        let progress = tokio::sync::Mutex::new(j.subscribe_progress());
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        jobs()
            .lock()
            .unwrap()
            .insert(id, Arc::new(JobHandle { job: j, progress }));
        Ok(serde_json::json!(id))
    }
);

json_input!(
    ne_query_open_json,
    "Opens a database query and returns its native handle as JSON.",
    |json: &str| {
        let request = serde_json::from_str(json)
            .map_err(|_| ExportError::new("INVALID_CONFIG", "Invalid query JSON"))?;
        let q = runtime().block_on(QueryService::stream(request))?;
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let handle = QueryHandle {
            cancel: q.cancel_handle(),
            session: tokio::sync::Mutex::new(q),
        };
        queries().lock().unwrap().insert(id, Arc::new(handle));
        Ok(serde_json::json!(id))
    }
);

json_input!(
    ne_input_open_json,
    "Opens an input stream and returns its native handle and input ID as JSON.",
    |json: &str| {
        let options: InputOptions = if json.trim().is_empty() {
            InputOptions::default()
        } else {
            serde_json::from_str(json)
                .map_err(|_| ExportError::new("INVALID_CONFIG", "Invalid input options JSON"))?
        };
        let input_id = InputService::open(options)?;
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        inputs().lock().unwrap().insert(id, input_id.clone());
        Ok(serde_json::json!({ "handle": id, "inputId": input_id }))
    }
);

/// Writes a JSON batch of records to an input stream, waiting for channel capacity.
///
/// # Safety
///
/// A non-null `data` must address `len` initialized bytes in one allocation, readable
/// and unmodified for this call. UTF-8 is validated at runtime. A non-null `out`
/// must be aligned and exclusively writable for one `NeBuffer`. Release any previous
/// allocation before calling, and free the returned buffer with [`ne_buffer_free`]
/// on both success and failure. Null output returns failure without a buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ne_input_write_batch_json(
    id: u64,
    data: *const u8,
    len: usize,
    out: *mut NeBuffer,
) -> i32 {
    // SAFETY: the caller upholds the documented input and output pointer contracts.
    unsafe {
        response(out, || {
            let json = text(data, len)?;
            let stream_id = input_id(id)?;
            runtime().block_on(InputService::write_batch_json(&stream_id, json))?;
            Ok(serde_json::json!(true))
        })
    }
}

macro_rules! handle_call {
    ($name:ident, $summary:literal, $handler:expr) => {
        #[doc = $summary]
        ///
        /// # Safety
        ///
        /// A non-null `out` must be aligned and exclusively writable for one `NeBuffer`.
        /// Release any allocation previously stored there. Free the returned buffer
        /// with `ne_buffer_free` on both success and failure. Null output returns
        /// failure without a buffer.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(id: u64, out: *mut NeBuffer) -> i32 {
            // SAFETY: the caller upholds the output pointer and ownership requirements.
            unsafe { response(out, || ($handler)(id)) }
        }
    };
}

handle_call!(
    ne_job_status_json,
    "Returns a snapshot of the export job status as JSON.",
    |id| Ok(serde_json::to_value(job(id)?.status()).unwrap())
);

handle_call!(
    ne_job_next_event_json,
    "Waits for a job status change or returns JSON null after completion.",
    |id| {
        let h = job_handle(id)?;
        runtime().block_on(async {
            let mut rx = h
                .progress
                .try_lock()
                .map_err(|_| ExportError::new("BUSY", "Concurrent event polling is unsupported"))?;
            let current = rx.borrow_and_update().clone();
            if matches!(current.phase.as_str(), "succeeded" | "failed" | "cancelled")
                && h.job.try_result().is_some()
            {
                return Ok(serde_json::Value::Null);
            }
            let _ = rx.changed().await;
            Ok(serde_json::to_value(rx.borrow_and_update().clone()).unwrap())
        })
    }
);

handle_call!(
    ne_job_result_json,
    "Waits for the export job result and returns its report as JSON.",
    |id| Ok(serde_json::to_value(runtime().block_on(job(id)?.result())?).unwrap())
);

handle_call!(
    ne_job_cancel,
    "Requests cooperative cancellation of an export job.",
    |id| {
        job(id)?.cancel();
        Ok(serde_json::json!(true))
    }
);

handle_call!(
    ne_job_release,
    "Releases a native job handle and requests cancellation.",
    |id| {
        let j = jobs()
            .lock()
            .unwrap()
            .remove(&id)
            .ok_or_else(invalid_handle)?;
        j.job.cancel();
        Ok(serde_json::json!(true))
    }
);

handle_call!(
    ne_query_schema_json,
    "Returns the query result column labels as JSON.",
    |id| {
        let q = query(id)?;
        let q = q
            .session
            .try_lock()
            .map_err(|_| ExportError::new("BUSY", "Query is being read"))?;
        Ok(serde_json::json!(q.schema()))
    }
);

handle_call!(
    ne_query_next_json,
    "Waits for the next query row or returns JSON null at end of stream.",
    |id| {
        let q = query(id)?;
        runtime().block_on(async {
            let mut q = q
                .session
                .try_lock()
                .map_err(|_| ExportError::new("BUSY", "Concurrent reads are unsupported"))?;
            Ok(serde_json::json!(q.next().await?))
        })
    }
);

handle_call!(
    ne_query_cancel,
    "Requests abortion of the query producer task.",
    |id| {
        let q = query(id)?;
        q.cancel.abort();
        Ok(serde_json::json!(true))
    }
);

handle_call!(
    ne_query_release,
    "Releases a native query handle and requests producer task abortion.",
    |id| {
        let q = queries()
            .lock()
            .unwrap()
            .remove(&id)
            .ok_or_else(invalid_handle)?;
        q.cancel.abort();
        Ok(serde_json::json!(true))
    }
);

handle_call!(
    ne_input_finish,
    "Closes an input stream to new records and allows queued records to drain.",
    |id| {
        let stream_id = input_id(id)?;
        InputService::finish(&stream_id)?;
        Ok(serde_json::json!(true))
    }
);

handle_call!(
    ne_input_release,
    "Releases a native input handle and cancels an unfinished stream.",
    |id| {
        let stream_id = inputs()
            .lock()
            .unwrap()
            .remove(&id)
            .ok_or_else(invalid_handle)?;
        let _ = InputService::release(&stream_id);
        Ok(serde_json::json!(true))
    }
);

/// Writes the generated export request schema as an owned JSON response.
///
/// # Safety
///
/// A non-null `out` must be aligned and exclusively writable for one `NeBuffer`.
/// Release any allocation previously stored there. Free the returned buffer with
/// [`ne_buffer_free`] on both success and failure. Null returns failure without a buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ne_schema_json(out: *mut NeBuffer) -> i32 {
    // SAFETY: the caller upholds the output pointer and ownership requirements.
    unsafe { response(out, || Ok(request_schema())) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_handle_returns_owned_error_buffer() {
        let mut out = NeBuffer::default();
        // SAFETY: output is writable; the returned allocation is read before being freed once.
        unsafe {
            assert_eq!(ne_job_status_json(u64::MAX, &mut out), 1);
            let bytes = std::slice::from_raw_parts(out.data, out.len);
            let value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            assert_eq!(value["code"], "INVALID_HANDLE");
            ne_buffer_free(out);
        }
    }
    #[test]
    fn schema_capabilities_and_input_roundtrip() {
        let mut out = NeBuffer::default();
        // SAFETY: inputs remain live, outputs are writable, and each allocation is freed once.
        unsafe {
            assert_eq!(ne_schema_json(&mut out), 0);
            assert!(out.len > 0);
            ne_buffer_free(out);
            assert_eq!(ne_schema_json(std::ptr::null_mut()), 1);

            let mut cap_out = NeBuffer::default();
            assert_eq!(ne_capabilities_json(&mut cap_out), 0);
            let cap_bytes = std::slice::from_raw_parts(cap_out.data, cap_out.len);
            let cap_val: serde_json::Value = serde_json::from_slice(cap_bytes).unwrap();
            assert_eq!(cap_val["abiVersion"], 1);
            ne_buffer_free(cap_out);

            let mut in_out = NeBuffer::default();
            let opts = b"{}";
            assert_eq!(
                ne_input_open_json(opts.as_ptr(), opts.len(), &mut in_out),
                0
            );
            let in_val: serde_json::Value =
                serde_json::from_slice(std::slice::from_raw_parts(in_out.data, in_out.len))
                    .unwrap();
            let handle = in_val["handle"].as_u64().unwrap();
            ne_buffer_free(in_out);

            let mut rel_out = NeBuffer::default();
            assert_eq!(ne_input_release(handle, &mut rel_out), 0);
            ne_buffer_free(rel_out);
        }
    }
}

//! Stable C ABI bindings.
use native_excel::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::runtime::Runtime;

#[repr(C)]
#[derive(Default)]
pub struct NeBuffer {
    pub data: *mut u8,
    pub len: usize,
}
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static JOBS: OnceLock<Mutex<BTreeMap<u64, ExportJob>>> = OnceLock::new();
struct QueryHandle {
    session: tokio::sync::Mutex<QuerySession>,
    cancel: tokio::task::AbortHandle,
}
type Sessions = Mutex<BTreeMap<u64, Arc<QueryHandle>>>;
static QUERIES: OnceLock<Sessions> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| Runtime::new().expect("runtime initialization"))
}
fn jobs() -> &'static Mutex<BTreeMap<u64, ExportJob>> {
    JOBS.get_or_init(Default::default)
}
fn queries() -> &'static Sessions {
    QUERIES.get_or_init(Default::default)
}
fn invalid_handle() -> ExportError {
    ExportError::new("INVALID_HANDLE", "Invalid or released native handle")
}
fn job(id: u64) -> Result<ExportJob> {
    jobs()
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(invalid_handle)
}
fn query(id: u64) -> Result<Arc<QueryHandle>> {
    queries()
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(invalid_handle)
}
// SAFETY: callers must provide readable input and a writable, aligned output pointer.
unsafe fn text<'a>(data: *const u8, len: usize) -> Result<&'a str> {
    if data.is_null() || len > 32 * 1024 * 1024 {
        return Err(ExportError::new(
            "INVALID_CONFIG",
            "Null input or input exceeds 32 MiB",
        ));
    }
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(data, len) })
        .map_err(|_| ExportError::new("INVALID_CONFIG", "Expected UTF-8"))
}
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
    unsafe {
        *out = NeBuffer {
            data: bytes.as_mut_ptr(),
            len: bytes.len(),
        };
    }
    std::mem::forget(bytes);
    status
}
#[unsafe(no_mangle)]
pub extern "C" fn ne_abi_version() -> u32 {
    1
}
/// # Safety
/// Buffer must be an unreleased value returned by this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ne_buffer_free(buffer: NeBuffer) {
    if !buffer.data.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(buffer.data, buffer.len)) });
    }
}
macro_rules! json_input {
    ($name:ident,$handler:expr) => {
        /// # Safety
        /// Input must be readable UTF-8; output must point to an empty writable buffer.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(data: *const u8, len: usize, out: *mut NeBuffer) -> i32 {
            unsafe {
                response(out, || {
                    let json = text(data, len)?;
                    ($handler)(json)
                })
            }
        }
    };
}
json_input!(ne_validate_json, |json: &str| {
    ExcelExporter::validate(&parse_request(json)?)?;
    Ok(serde_json::json!(true))
});
json_input!(ne_export_start_json, |json: &str| {
    let request = parse_request(json)?;
    let _guard = runtime().enter();
    let j = ExcelExporter::start(request)?;
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    jobs().lock().unwrap().insert(id, j);
    Ok(serde_json::json!(id))
});
json_input!(ne_query_open_json, |json: &str| {
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
});
macro_rules! handle_call {
    ($name:ident,$handler:expr) => {
        /// # Safety
        /// Output must point to an empty writable buffer.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(id: u64, out: *mut NeBuffer) -> i32 {
            unsafe { response(out, || ($handler)(id)) }
        }
    };
}
handle_call!(ne_job_status_json, |id| Ok(serde_json::to_value(
    job(id)?.status()
)
.unwrap()));
handle_call!(ne_job_result_json, |id| Ok(serde_json::to_value(
    runtime().block_on(job(id)?.result())?
)
.unwrap()));
handle_call!(ne_job_cancel, |id| {
    job(id)?.cancel();
    Ok(serde_json::json!(true))
});
handle_call!(ne_job_release, |id| {
    let j = jobs()
        .lock()
        .unwrap()
        .remove(&id)
        .ok_or_else(invalid_handle)?;
    j.cancel();
    Ok(serde_json::json!(true))
});
handle_call!(ne_query_schema_json, |id| {
    let q = query(id)?;
    let q = q
        .session
        .try_lock()
        .map_err(|_| ExportError::new("BUSY", "Query is being read"))?;
    Ok(serde_json::json!(q.schema()))
});
handle_call!(ne_query_next_json, |id| {
    let q = query(id)?;
    runtime().block_on(async {
        let mut q = q
            .session
            .try_lock()
            .map_err(|_| ExportError::new("BUSY", "Concurrent reads are unsupported"))?;
        Ok(serde_json::json!(q.next().await?))
    })
});
handle_call!(ne_query_release, |id| {
    let q = queries()
        .lock()
        .unwrap()
        .remove(&id)
        .ok_or_else(invalid_handle)?;
    q.cancel.abort();
    Ok(serde_json::json!(true))
});
/// # Safety
/// Output must point to an empty writable buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ne_schema_json(out: *mut NeBuffer) -> i32 {
    unsafe { response(out, || Ok(request_schema())) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_handle_returns_owned_error_buffer() {
        let mut out = NeBuffer::default();
        unsafe {
            assert_eq!(ne_job_status_json(u64::MAX, &mut out), 1);
            let bytes = std::slice::from_raw_parts(out.data, out.len);
            let value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            assert_eq!(value["code"], "INVALID_HANDLE");
            ne_buffer_free(out);
        }
    }
    #[test]
    fn schema_roundtrip_and_null_output() {
        let mut out = NeBuffer::default();
        unsafe {
            assert_eq!(ne_schema_json(&mut out), 0);
            assert!(out.len > 0);
            ne_buffer_free(out);
            assert_eq!(ne_schema_json(std::ptr::null_mut()), 1);
        }
    }
}

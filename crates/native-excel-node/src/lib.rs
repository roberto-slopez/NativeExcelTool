//! Node-API bindings.
use napi::bindgen_prelude::*;
use napi_derive::napi;
use native_excel as core;
use std::sync::Arc;

fn error(e: core::ExportError) -> napi::Error {
    napi::Error::from_reason(serde_json::to_string(&e).unwrap_or_else(|_| e.to_string()))
}
#[napi]
pub fn validate_json(json: String) -> Result<()> {
    core::ExcelExporter::validate(&core::parse_request(&json).map_err(error)?).map_err(error)
}
#[napi]
pub fn schema_json() -> String {
    core::request_schema().to_string()
}
#[napi]
pub fn capabilities_json() -> String {
    serde_json::to_string(&core::ExcelExporter::capabilities()).unwrap()
}
#[napi]
pub async fn export_json(json: String) -> Result<String> {
    let report = core::ExcelExporter::export(core::parse_request(&json).map_err(error)?)
        .await
        .map_err(error)?;
    Ok(serde_json::to_string(&report).unwrap())
}
#[napi]
pub struct ExportJob {
    inner: core::ExportJob,
    progress: Arc<tokio_mutex::Mutex<napi::tokio::sync::watch::Receiver<core::JobStatus>>>,
}
#[napi]
impl ExportJob {
    #[napi]
    pub fn cancel(&self) {
        self.inner.cancel();
    }
    #[napi]
    pub fn status_json(&self) -> String {
        serde_json::to_string(&self.inner.status()).unwrap()
    }
    #[napi]
    pub async fn next_event_json(&self) -> Result<Option<String>> {
        let mut rx = self
            .progress
            .try_lock()
            .map_err(|_| napi::Error::from_reason("Concurrent event reads are unsupported"))?;
        let current = rx.borrow_and_update().clone();
        if matches!(current.phase.as_str(), "succeeded" | "failed" | "cancelled")
            && self.inner.try_result().is_some()
        {
            return Ok(None);
        }
        let _ = rx.changed().await;
        Ok(Some(
            serde_json::to_string(&rx.borrow_and_update().clone()).unwrap(),
        ))
    }
    #[napi]
    pub async fn result_json(&self) -> Result<String> {
        Ok(serde_json::to_string(&self.inner.result().await.map_err(error)?).unwrap())
    }
}
#[napi]
pub async fn start_export_json(json: String) -> Result<ExportJob> {
    let inner =
        core::ExcelExporter::start(core::parse_request(&json).map_err(error)?).map_err(error)?;
    let progress = Arc::new(tokio_mutex::Mutex::new(inner.subscribe_progress()));
    Ok(ExportJob { inner, progress })
}
#[napi]
pub struct InputStream {
    id: String,
}
#[napi]
impl InputStream {
    #[napi]
    pub fn input_id(&self) -> String {
        self.id.clone()
    }
    #[napi]
    pub async fn write_batch_json(&self, json: String) -> Result<()> {
        core::InputService::write_batch_json(&self.id, &json)
            .await
            .map_err(error)
    }
    #[napi]
    pub fn finish(&self) -> Result<()> {
        core::InputService::finish(&self.id).map_err(error)
    }
    #[napi]
    pub fn close(&self) {
        let _ = core::InputService::release(&self.id);
    }
}
#[napi]
pub fn open_input_json(json: Option<String>) -> Result<InputStream> {
    let options: core::InputOptions = match json {
        Some(s) if !s.trim().is_empty() => serde_json::from_str(&s)
            .map_err(|_| napi::Error::from_reason("Invalid input options JSON"))?,
        _ => core::InputOptions::default(),
    };
    let id = core::InputService::open(options).map_err(error)?;
    Ok(InputStream { id })
}
#[napi]
pub struct QueryStream {
    inner: Arc<tokio_mutex::Mutex<Option<core::QuerySession>>>,
    schema: String,
    cancel: tokio::task::AbortHandle,
}
// Tokio is re-exported by napi for native async execution.
mod tokio_mutex {
    pub use napi::tokio::sync::Mutex;
}
#[napi]
impl QueryStream {
    #[napi]
    pub fn schema_json(&self) -> String {
        self.schema.clone()
    }
    #[napi]
    pub async fn next_json(&self) -> Result<Option<String>> {
        let mut lock = self
            .inner
            .try_lock()
            .map_err(|_| napi::Error::from_reason("Concurrent reads are unsupported"))?;
        let s = lock
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("Stream closed"))?;
        match s.next().await.map_err(error)? {
            Some(row) => Ok(Some(serde_json::to_string(&row).unwrap())),
            None => {
                lock.take();
                Ok(None)
            }
        }
    }
    #[napi]
    pub async fn close(&self) {
        self.cancel.abort();
        self.inner.lock().await.take();
    }
}
#[napi]
pub async fn query_stream_json(json: String) -> Result<QueryStream> {
    let req =
        serde_json::from_str(&json).map_err(|_| napi::Error::from_reason("Invalid query JSON"))?;
    let stream = core::QueryService::stream(req).await.map_err(error)?;
    Ok(QueryStream {
        schema: serde_json::to_string(stream.schema()).unwrap(),
        cancel: stream.cancel_handle(),
        inner: Arc::new(tokio_mutex::Mutex::new(Some(stream))),
    })
}

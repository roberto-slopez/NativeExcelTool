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
pub async fn export_json(json: String) -> Result<String> {
    let report = core::ExcelExporter::export(core::parse_request(&json).map_err(error)?)
        .await
        .map_err(error)?;
    Ok(serde_json::to_string(&report).unwrap())
}
#[napi]
pub struct ExportJob {
    inner: core::ExportJob,
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
    pub async fn result_json(&self) -> Result<String> {
        Ok(serde_json::to_string(&self.inner.result().await.map_err(error)?).unwrap())
    }
}
#[napi]
pub async fn start_export_json(json: String) -> Result<ExportJob> {
    Ok(ExportJob {
        inner: core::ExcelExporter::start(core::parse_request(&json).map_err(error)?)
            .map_err(error)?,
    })
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

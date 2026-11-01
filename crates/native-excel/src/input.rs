//! Incremental caller-provided record streams with bounded channels and backpressure.
use crate::{ExportError, Result, Row};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::mpsc;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct InputOptions {
    pub queue_rows: usize,
    pub max_batch_bytes: usize,
    pub max_row_bytes: usize,
}

impl Default for InputOptions {
    fn default() -> Self {
        Self {
            queue_rows: 16,
            max_batch_bytes: 4 * 1024 * 1024,
            max_row_bytes: 1024 * 1024,
        }
    }
}

struct RegisteredInput {
    tx: Option<mpsc::Sender<Result<Row>>>,
    rx: Option<mpsc::Receiver<Result<Row>>>,
    options: InputOptions,
    finished: bool,
}

static INPUT_REGISTRY: OnceLock<Mutex<BTreeMap<String, Arc<Mutex<RegisteredInput>>>>> =
    OnceLock::new();
static NEXT_INPUT_ID: AtomicU64 = AtomicU64::new(1);

fn registry() -> &'static Mutex<BTreeMap<String, Arc<Mutex<RegisteredInput>>>> {
    INPUT_REGISTRY.get_or_init(Default::default)
}

fn validate_options(options: &InputOptions) -> Result<()> {
    if options.queue_rows == 0
        || options.queue_rows > 4096
        || options.max_batch_bytes == 0
        || options.max_batch_bytes > 64 * 1024 * 1024
        || options.max_row_bytes == 0
        || options.max_row_bytes > 64 * 1024 * 1024
    {
        return Err(ExportError::invalid(
            "Invalid input stream limits: queueRows must be 1..4096 and byte limits 1..64MiB",
        ));
    }
    Ok(())
}

/// Handle for incremental caller-provided records.
/// Dropping an unfinished session aborts the stream and signals cancellation to any reader.
pub struct InputSession {
    id: String,
    entry: Arc<Mutex<RegisteredInput>>,
}

impl InputSession {
    pub fn open(options: InputOptions) -> Result<Self> {
        validate_options(&options)?;
        let id = format!("input-{}", NEXT_INPUT_ID.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = mpsc::channel(options.queue_rows);
        let entry = Arc::new(Mutex::new(RegisteredInput {
            tx: Some(tx),
            rx: Some(rx),
            options,
            finished: false,
        }));
        registry().lock().unwrap().insert(id.clone(), entry.clone());
        Ok(Self { id, entry })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub async fn write_batch(&self, rows: Vec<Row>) -> Result<()> {
        write_batch_internal(&self.entry, rows).await
    }

    pub fn finish(self) -> Result<()> {
        finish_internal(&self.id, &self.entry)
    }

    pub fn close(self) {
        // Drop triggers cleanup
    }
}

impl Drop for InputSession {
    fn drop(&mut self) {
        release_internal(&self.id, &self.entry);
    }
}

pub struct InputService;

impl InputService {
    pub fn open(options: InputOptions) -> Result<String> {
        validate_options(&options)?;
        let id = format!("input-{}", NEXT_INPUT_ID.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = mpsc::channel(options.queue_rows);
        let entry = Arc::new(Mutex::new(RegisteredInput {
            tx: Some(tx),
            rx: Some(rx),
            options,
            finished: false,
        }));
        registry().lock().unwrap().insert(id.clone(), entry);
        Ok(id)
    }

    pub async fn write_batch(id: &str, rows: Vec<Row>) -> Result<()> {
        let entry = get_entry(id)?;
        write_batch_internal(&entry, rows).await
    }

    pub async fn write_batch_json(id: &str, json: &str) -> Result<()> {
        let entry = get_entry(id)?;
        let max_batch = entry.lock().unwrap().options.max_batch_bytes;
        if json.len() > max_batch {
            return Err(ExportError::new(
                "LIMIT_EXCEEDED",
                "Input batch exceeds maxBatchBytes",
            ));
        }
        let rows: Vec<Row> = serde_json::from_str(json)
            .map_err(|_| ExportError::invalid("Invalid input batch JSON array of row objects"))?;
        write_batch_internal(&entry, rows).await
    }

    pub fn finish(id: &str) -> Result<()> {
        let entry = get_entry(id)?;
        finish_internal(id, &entry)
    }

    pub fn release(id: &str) -> Result<()> {
        let entry =
            registry().lock().unwrap().remove(id).ok_or_else(|| {
                ExportError::new("INVALID_HANDLE", "Invalid or released input ID")
            })?;
        release_internal(id, &entry);
        Ok(())
    }

    pub(crate) fn exists(id: &str) -> bool {
        registry()
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|e| e.lock().unwrap().rx.is_some())
    }

    pub(crate) fn take_receiver(id: &str) -> Result<mpsc::Receiver<Result<Row>>> {
        let entry = get_entry(id)?;
        let mut guard = entry.lock().unwrap();
        let rx = guard.rx.take().ok_or_else(|| {
            ExportError::new(
                "INVALID_HANDLE",
                "Input stream already consumed or released",
            )
        })?;
        let should_remove = guard.finished && guard.tx.is_none();
        drop(guard);
        if should_remove {
            registry().lock().unwrap().remove(id);
        }
        Ok(rx)
    }
}

fn get_entry(id: &str) -> Result<Arc<Mutex<RegisteredInput>>> {
    registry()
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or_else(|| ExportError::new("INVALID_HANDLE", "Invalid or released input ID"))
}

async fn write_batch_internal(entry: &Arc<Mutex<RegisteredInput>>, rows: Vec<Row>) -> Result<()> {
    let (tx, options) = {
        let guard = entry.lock().unwrap();
        if guard.finished {
            return Err(ExportError::new(
                "INVALID_HANDLE",
                "Input stream is already finished",
            ));
        }
        let tx = guard
            .tx
            .clone()
            .ok_or_else(|| ExportError::new("INVALID_HANDLE", "Input stream is closed"))?;
        (tx, guard.options.clone())
    };
    let mut batch_bytes = 0usize;
    for row in rows {
        let encoded_len = serde_json::to_vec(&row)
            .map_err(|_| ExportError::new("CONVERSION_FAILED", "Failed to serialize input row"))?
            .len();
        if encoded_len > options.max_row_bytes {
            return Err(ExportError::new(
                "LIMIT_EXCEEDED",
                "Input row exceeds maxRowBytes",
            ));
        }
        batch_bytes = batch_bytes.saturating_add(encoded_len);
        if batch_bytes > options.max_batch_bytes {
            return Err(ExportError::new(
                "LIMIT_EXCEEDED",
                "Input batch exceeds maxBatchBytes",
            ));
        }
        tx.send(Ok(row))
            .await
            .map_err(|_| ExportError::new("CANCELLED", "Export consumer closed input stream"))?;
    }
    Ok(())
}

fn finish_internal(id: &str, entry: &Arc<Mutex<RegisteredInput>>) -> Result<()> {
    let should_remove = {
        let mut guard = entry.lock().unwrap();
        guard.finished = true;
        guard.tx.take();
        guard.rx.is_none()
    };
    if should_remove {
        registry().lock().unwrap().remove(id);
    }
    Ok(())
}

fn release_internal(id: &str, entry: &Arc<Mutex<RegisteredInput>>) {
    let tx = {
        let mut guard = entry.lock().unwrap();
        if guard.finished {
            None
        } else {
            guard.rx.take();
            guard.tx.take()
        }
    };
    if let Some(tx) = tx {
        let _ = tx.try_send(Err(ExportError::new(
            "CANCELLED",
            "Caller input session released before finish",
        )));
    }
    registry().lock().unwrap().remove(id);
}

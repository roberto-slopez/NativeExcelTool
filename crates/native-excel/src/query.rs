use crate::*;
use futures_util::{StreamExt, pin_mut};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tokio_util::compat::TokioAsyncWriteCompatExt;

/// Owns a database query producer and its bounded row receiver.
///
/// Dropping the session requests task abortion. The connection is released when
/// the producer future is dropped; dropping the session does not await cleanup.
pub struct QuerySession {
    schema: Vec<String>,
    rx: mpsc::Receiver<Result<Row>>,
    task: JoinHandle<()>,
}
impl QuerySession {
    pub fn cancel_handle(&self) -> tokio::task::AbortHandle {
        self.task.abort_handle()
    }
    pub fn schema(&self) -> &[String] {
        &self.schema
    }
    pub async fn next(&mut self) -> Result<Option<Row>> {
        self.rx.recv().await.transpose()
    }
    pub async fn close(self) {
        self.task.abort();
    }
}
impl Drop for QuerySession {
    fn drop(&mut self) {
        self.task.abort();
    }
}
pub struct QueryService;
/// Produces owned rows from a database connection through a bounded channel.
///
/// Implementations send the result schema before any rows and retain ownership
/// of driver connections and borrowed database streams inside the producer future.
pub trait QueryDriver: Send + Sync {
    fn produce<'a>(
        &'a self,
        request: QueryRequest,
        rows: &'a mpsc::Sender<Result<Row>>,
        schema: oneshot::Sender<Vec<String>>,
    ) -> futures_util::future::BoxFuture<'a, Result<()>>;
}
pub struct PostgresDriver;
pub struct SqlServerDriver;
impl QueryDriver for PostgresDriver {
    fn produce<'a>(
        &'a self,
        request: QueryRequest,
        rows: &'a mpsc::Sender<Result<Row>>,
        schema: oneshot::Sender<Vec<String>>,
    ) -> futures_util::future::BoxFuture<'a, Result<()>> {
        Box::pin(postgres(request, rows, schema))
    }
}
impl QueryDriver for SqlServerDriver {
    fn produce<'a>(
        &'a self,
        request: QueryRequest,
        rows: &'a mpsc::Sender<Result<Row>>,
        schema: oneshot::Sender<Vec<String>>,
    ) -> futures_util::future::BoxFuture<'a, Result<()>> {
        Box::pin(sqlserver(request, rows, schema))
    }
}
impl QueryService {
    pub async fn stream(request: QueryRequest) -> Result<QuerySession> {
        let driver: std::sync::Arc<dyn QueryDriver> = match request.dbms {
            Dbms::Postgresql => std::sync::Arc::new(PostgresDriver),
            Dbms::Sqlserver => std::sync::Arc::new(SqlServerDriver),
        };
        Self::stream_with(driver, request).await
    }
    pub async fn stream_with(
        driver: std::sync::Arc<dyn QueryDriver>,
        request: QueryRequest,
    ) -> Result<QuerySession> {
        let o = &request.options;
        if o.fetch_rows == 0
            || o.fetch_rows > 65536
            || o.queue_rows == 0
            || o.queue_rows > 4096
            || o.max_row_bytes == 0
            || o.max_row_bytes > 64 * 1024 * 1024
            || o.connect_timeout_ms == 0
            || o.query_timeout_ms == 0
        {
            return Err(ExportError::invalid("Invalid query limits or timeouts"));
        }
        let (tx, rx) = mpsc::channel(o.queue_rows);
        let (schema_tx, schema_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            let timeout = Duration::from_millis(request.options.query_timeout_ms);
            let result = tokio::time::timeout(timeout, async {
                driver.produce(request, &tx, schema_tx).await
            })
            .await
            .unwrap_or_else(|_| Err(ExportError::new("TIMEOUT", "Query deadline exceeded")));
            if let Err(e) = result {
                let _ = tx.send(Err(e)).await;
            }
        });
        let mut guard = PendingTask(Some(task));
        match schema_rx.await {
            Ok(schema) => Ok(QuerySession {
                schema,
                rx,
                task: guard.0.take().unwrap(),
            }),
            Err(_) => {
                let mut rx = rx;
                let error = rx.recv().await.and_then(|r| r.err()).unwrap_or_else(|| {
                    ExportError::new("QUERY_FAILED", "Query ended before metadata")
                });
                Err(error)
            }
        }
    }
}
struct PendingTask(Option<JoinHandle<()>>);
impl Drop for PendingTask {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}
struct AbortOnDrop(JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
fn db_error(_: impl std::fmt::Display) -> ExportError {
    ExportError::new(
        "QUERY_FAILED",
        "Database operation failed; inspect server diagnostics without exposing credentials",
    )
}
fn schema(names: Vec<String>) -> Result<Vec<String>> {
    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    if names.is_empty() || unique.len() != names.len() {
        return Err(ExportError::new(
            "SCHEMA_MISMATCH",
            "Expected nonempty, unique result labels; use SQL aliases",
        ));
    }
    Ok(names)
}
async fn emit(tx: &mpsc::Sender<Result<Row>>, row: Row, max: usize) -> Result<()> {
    if serde_json::to_vec(&row).map_err(db_error)?.len() > max {
        return Err(ExportError::new(
            "LIMIT_EXCEEDED",
            "Decoded row exceeds maxRowBytes",
        ));
    }
    tx.send(Ok(row))
        .await
        .map_err(|_| ExportError::new("CANCELLED", "Consumer closed"))
}
fn finite_float(value: Option<f64>) -> Result<Value> {
    if value.is_some_and(|v| !v.is_finite()) {
        return Err(ExportError::new(
            "CONVERSION_FAILED",
            "Non-finite database float",
        ));
    }
    Ok(json!(value))
}
async fn postgres(
    request: QueryRequest,
    tx: &mpsc::Sender<Result<Row>>,
    schema_tx: oneshot::Sender<Vec<String>>,
) -> Result<()> {
    use tokio_postgres::{config::SslMode, types::ToSql};
    let mut config: tokio_postgres::Config = request
        .connection_string
        .parse()
        .map_err(|_| ExportError::invalid("Invalid PostgreSQL connection string"))?;
    if !request.options.allow_plaintext {
        config.ssl_mode(SslMode::Require);
    }
    config.connect_timeout(Duration::from_millis(request.options.connect_timeout_ms));
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let tls = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_postgres_rustls::MakeRustlsConnect::new(tls);
    let (mut client, connection) = tokio::time::timeout(
        Duration::from_millis(request.options.connect_timeout_ms),
        config.connect(connector),
    )
    .await
    .map_err(|_| ExportError::new("TIMEOUT", "Connect timeout"))?
    .map_err(db_error)?;
    let _connection = AbortOnDrop(tokio::spawn(async move {
        let _ = connection.await;
    }));
    let params: Vec<Box<dyn ToSql + Sync + Send>> = request
        .parameters
        .into_iter()
        .map(|p| match p {
            Parameter::Text(v) => Box::new(v) as Box<dyn ToSql + Sync + Send>,
            Parameter::Int32(v) => Box::new(v),
            Parameter::Int64(v) => Box::new(v),
            Parameter::Float64(v) => Box::new(v),
            Parameter::Boolean(v) => Box::new(v),
        })
        .collect();
    let transaction = client
        .build_transaction()
        .read_only(true)
        .start()
        .await
        .map_err(db_error)?;
    let statement = transaction
        .prepare(&request.query)
        .await
        .map_err(db_error)?;
    if statement.params().len() != params.len() {
        return Err(ExportError::invalid("Query parameter count mismatch"));
    }
    let names = schema(
        statement
            .columns()
            .iter()
            .map(|c| c.name().to_owned())
            .collect(),
    )?;
    let _ = schema_tx.send(names.clone());
    let refs: Vec<_> = params.iter().map(|p| &**p as &(dyn ToSql + Sync)).collect();
    let portal = transaction
        .bind(&statement, &refs)
        .await
        .map_err(db_error)?;
    loop {
        let stream = transaction
            .query_portal_raw(&portal, request.options.fetch_rows as i32)
            .await
            .map_err(db_error)?;
        let mut fetched = 0;
        pin_mut!(stream);
        while let Some(row) = stream.next().await {
            fetched += 1;
            let row = row.map_err(db_error)?;
            let mut output = Row::new();
            for (i, name) in names.iter().enumerate() {
                let ty = row.columns()[i].type_().name();
                macro_rules! get {
                    ($t:ty) => {
                        row.try_get::<_, Option<$t>>(i).map_err(db_error)?
                    };
                }
                let value = match ty {
                    "bool" => json!(get!(bool)),
                    "int2" => json!(get!(i16)),
                    "int4" => json!(get!(i32)),
                    "int8" => get!(i64)
                        .map(|v| json!({"$type":"int64","value":v.to_string()}))
                        .unwrap_or(Value::Null),
                    "float4" => finite_float(get!(f32).map(f64::from))?,
                    "float8" => finite_float(get!(f64))?,
                    "numeric" => json!(get!(rust_decimal::Decimal).map(|v| v.to_string())),
                    "text" | "varchar" | "bpchar" | "name" => json!(get!(String)),
                    "uuid" => json!(get!(uuid::Uuid).map(|v| v.to_string())),
                    "date" => json!(get!(chrono::NaiveDate).map(|v| v.to_string())),
                    "timestamp" => json!(get!(chrono::NaiveDateTime).map(|v| v.to_string())),
                    "timestamptz" => {
                        json!(get!(chrono::DateTime<chrono::Utc>).map(|v| v.to_rfc3339()))
                    }
                    "time" => json!(get!(chrono::NaiveTime).map(|v| v.to_string())),
                    "json" | "jsonb" => get!(Value)
                        .map(|v| Value::String(v.to_string()))
                        .unwrap_or(Value::Null),
                    _ => {
                        return Err(ExportError::new(
                            "UNSUPPORTED_TYPE",
                            format!("PostgreSQL type {ty}; select an explicit text cast"),
                        ));
                    }
                };
                output.insert(name.clone(), value);
            }
            emit(tx, output, request.options.max_row_bytes).await?;
        }
        if fetched < request.options.fetch_rows {
            break;
        }
    }
    transaction.commit().await.map_err(db_error)?;
    Ok(())
}
async fn sqlserver(
    request: QueryRequest,
    tx: &mpsc::Sender<Result<Row>>,
    schema_tx: oneshot::Sender<Vec<String>>,
) -> Result<()> {
    let mut config = tiberius::Config::from_ado_string(&request.connection_string)
        .map_err(|_| ExportError::invalid("Invalid SQL Server connection string"))?;
    let normalized: String = request
        .connection_string
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if normalized.contains("trustservercertificate=true")
        || normalized.contains("trustservercertificate=yes")
    {
        return Err(ExportError::invalid(
            "Unverified SQL Server certificates are not supported",
        ));
    }
    if !request.options.allow_plaintext {
        config.encryption(tiberius::EncryptionLevel::Required);
    }
    let connect = async {
        let tcp = tokio::net::TcpStream::connect(config.get_addr())
            .await
            .map_err(db_error)?;
        tcp.set_nodelay(true).map_err(db_error)?;
        tiberius::Client::connect(config, tcp.compat_write())
            .await
            .map_err(db_error)
    };
    let mut client = tokio::time::timeout(
        Duration::from_millis(request.options.connect_timeout_ms),
        connect,
    )
    .await
    .map_err(|_| ExportError::new("TIMEOUT", "Connect timeout"))??;
    let mut query = tiberius::Query::new(request.query);
    for p in request.parameters {
        match p {
            Parameter::Text(v) => query.bind(v),
            Parameter::Int32(v) => query.bind(v),
            Parameter::Int64(v) => query.bind(v),
            Parameter::Float64(v) => query.bind(v),
            Parameter::Boolean(v) => query.bind(v),
        };
    }
    let mut stream = query.query(&mut client).await.map_err(db_error)?;
    let mut names = vec![];
    let mut schema_tx = Some(schema_tx);
    while let Some(item) = stream.next().await {
        match item.map_err(db_error)? {
            tiberius::QueryItem::Metadata(meta) => {
                if schema_tx.is_none() {
                    return Err(ExportError::new(
                        "SCHEMA_MISMATCH",
                        "Multiple result sets are unsupported",
                    ));
                }
                names = schema(meta.columns().iter().map(|c| c.name().to_owned()).collect())?;
                let _ = schema_tx.take().unwrap().send(names.clone());
            }
            tiberius::QueryItem::Row(row) => {
                use tiberius::ColumnData as C;
                let mut output = Row::new();
                for (name, v) in names.iter().zip(row) {
                    let value = match v {
                        C::U8(v) => json!(v),
                        C::I16(v) => json!(v),
                        C::I32(v) => json!(v),
                        C::I64(v) => v
                            .map(|v| json!({"$type":"int64","value":v.to_string()}))
                            .unwrap_or(Value::Null),
                        C::F32(v) => finite_float(v.map(f64::from))?,
                        C::F64(v) => finite_float(v)?,
                        C::Bit(v) => json!(v),
                        C::String(v) => json!(v),
                        C::Guid(v) => json!(v.map(|v| v.to_string())),
                        C::Numeric(v) => json!(v.map(|v| v.to_string())),
                        C::Date(_) => json!(
                            <chrono::NaiveDate as tiberius::FromSql>::from_sql(&v)
                                .map_err(db_error)?
                                .map(|v| v.to_string())
                        ),
                        C::Time(_) => json!(
                            <chrono::NaiveTime as tiberius::FromSql>::from_sql(&v)
                                .map_err(db_error)?
                                .map(|v| v.to_string())
                        ),
                        C::DateTime(_) | C::SmallDateTime(_) | C::DateTime2(_) => json!(
                            <chrono::NaiveDateTime as tiberius::FromSql>::from_sql(&v)
                                .map_err(db_error)?
                                .map(|v| v.to_string())
                        ),
                        C::DateTimeOffset(_) => json!(
                            <chrono::DateTime<chrono::FixedOffset> as tiberius::FromSql>::from_sql(
                                &v
                            )
                            .map_err(db_error)?
                            .map(|v| v.to_rfc3339())
                        ),
                        _ => {
                            return Err(ExportError::new(
                                "UNSUPPORTED_TYPE",
                                "SQL Server value requires explicit text conversion in SELECT",
                            ));
                        }
                    };
                    output.insert(name.clone(), value);
                }
                emit(tx, output, request.options.max_row_bytes).await?;
            }
        }
    }
    Ok(())
}

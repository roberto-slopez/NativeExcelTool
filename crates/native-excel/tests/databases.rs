use native_excel::*;
use serde_json::json;

async fn verify(dbms: &str, connection: &str, sql: &str) {
    let request:QueryRequest=serde_json::from_value(json!({"dbms":dbms,"connectionString":connection,"query":sql,"parameters":[{"type":"int32","value":42}],"options":{"queueRows":1,"fetchRows":1}})).unwrap();
    let mut stream = QueryService::stream(request).await.unwrap();
    assert_eq!(stream.schema(), &["id", "label"]);
    let row = stream.next().await.unwrap().unwrap();
    assert_eq!(row["id"], 42);
    assert_eq!(row["label"], "test");
    assert!(stream.next().await.unwrap().is_none());
    stream.close().await;
}

#[tokio::test]
#[ignore = "Requires NATIVE_EXCEL_POSTGRES connection string with trusted TLS"]
async fn postgres_parameterized_stream() {
    verify(
        "postgresql",
        &std::env::var("NATIVE_EXCEL_POSTGRES").expect("Set NATIVE_EXCEL_POSTGRES"),
        "SELECT $1::integer AS id, 'test'::text AS label",
    )
    .await;
}

#[tokio::test]
#[ignore = "Requires NATIVE_EXCEL_SQLSERVER connection string with trusted TLS"]
async fn sqlserver_parameterized_stream() {
    verify(
        "sqlserver",
        &std::env::var("NATIVE_EXCEL_SQLSERVER").expect("Set NATIVE_EXCEL_SQLSERVER"),
        "SELECT @P1 AS id, CAST('test' AS nvarchar(10)) AS label",
    )
    .await;
}

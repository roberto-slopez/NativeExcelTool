use futures_util::future::BoxFuture;
use native_excel::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::{mpsc, oneshot};
struct FakeDriver(Arc<AtomicUsize>);
impl QueryDriver for FakeDriver {
    fn produce<'a>(
        &'a self,
        _: QueryRequest,
        rows: &'a mpsc::Sender<Result<Row>>,
        schema: oneshot::Sender<Vec<String>>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let _ = schema.send(vec!["id".into()]);
            for i in 0..100 {
                rows.send(Ok([(String::from("id"), serde_json::json!(i))]
                    .into_iter()
                    .collect()))
                    .await
                    .map_err(|_| ExportError::new("CANCELLED", "Consumer closed"))?;
                self.0.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        })
    }
}
#[tokio::test]
async fn slow_consumer_backpressures_and_drop_stops_producer() {
    let produced = Arc::new(AtomicUsize::new(0));
    let req=serde_json::from_value(serde_json::json!({"dbms":"postgresql","connectionString":"unused","query":"unused","options":{"queueRows":1}})).unwrap();
    let mut session = QueryService::stream_with(Arc::new(FakeDriver(produced.clone())), req)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(produced.load(Ordering::SeqCst), 1);
    assert_eq!(session.next().await.unwrap().unwrap()["id"], 0);
    drop(session);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let after = produced.load(Ordering::SeqCst);
    assert!(after <= 2);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(produced.load(Ordering::SeqCst), after);
}

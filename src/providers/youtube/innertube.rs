use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use anyhow::Result;
use innertube_rs::{Session, SessionOptions};
use serde_json::{json, Value};

const INNERTUBE_TIMEOUT: Duration = Duration::from_secs(30);

static SESSION: OnceLock<Arc<Session>> = OnceLock::new();

pub(super) fn run_innertube<F, T>(future: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    let future = async move {
        tokio::time::timeout(INNERTUBE_TIMEOUT, future)
            .await
            .map_err(|_| anyhow::anyhow!("YouTube Music request timed out"))?
    };
    // Provider fetches mostly run on plain threads, but a few call sites are
    // nested inside the shared runtime; blocking on it from inside one of its
    // own worker threads would panic, so that case gets its own thread.
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::spawn(move || crate::providers::shared_runtime().block_on(future))
            .join()
            .map_err(|_| anyhow::anyhow!("YouTube Music worker panicked"))?
    } else {
        crate::providers::shared_runtime().block_on(future)
    }
}

pub(super) fn session() -> Result<Arc<Session>> {
    if let Some(session) = SESSION.get() {
        return Ok(session.clone());
    }
    let session = Arc::new(run_innertube(async {
        Session::create(SessionOptions {
            generate_session_locally: Some(true),
            retrieve_innertube_config: Some(false),
            // The raw parsers below match English literals ("plays", "Video
            // charts") and page-type strings; this is load-bearing, not just
            // display language.
            lang: Some("en-US".to_string()),
            ..Default::default()
        })
        .await
        .map_err(anyhow::Error::from)
    })?);
    Ok(SESSION.get_or_init(|| session).clone())
}

pub(super) async fn post_music(
    session: Arc<Session>,
    path: &'static str,
    body: Value,
) -> Result<Value> {
    let response = session.post_innertube_client("YTMUSIC", path, body).await?;
    let value: Value = response.json().await?;
    Ok(value)
}

pub(super) async fn raw_browse(session: Arc<Session>, browse_id: String) -> Result<Value> {
    post_music(session, "/browse", json!({ "browseId": browse_id })).await
}

pub(super) async fn raw_search(
    session: Arc<Session>,
    query: String,
    params: &'static str,
) -> Result<Value> {
    post_music(
        session,
        "/search",
        json!({ "query": query, "params": params }),
    )
    .await
}

pub(super) async fn raw_charts(session: Arc<Session>, country: &'static str) -> Result<Value> {
    post_music(
        session,
        "/browse",
        json!({
            "browseId": "FEmusic_charts",
            "formData": { "selectedValues": [country] },
        }),
    )
    .await
}

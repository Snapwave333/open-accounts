use futures::{
    stream::{SplitSink, StreamExt},
    SinkExt,
};
use openaccounts_core::{OpenAccountsError, Result};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    net::TcpStream,
    sync::{broadcast, oneshot},
    task::JoinHandle,
};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::debug;

pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(30);

type WsSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;
type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>;

#[derive(Debug, Clone)]
pub struct CdpEvent {
    pub method: String,
    pub params: Value,
}

pub struct CdpSession {
    sink: tokio::sync::Mutex<WsSink>,
    pending: PendingMap,
    next_id: AtomicU64,
    events: broadcast::Sender<CdpEvent>,
    closed: Arc<AtomicBool>,
    reader: JoinHandle<()>,
}

fn browser_err(msg: impl Into<String>) -> OpenAccountsError {
    OpenAccountsError::Browser(msg.into())
}

fn fail_all(pending: &PendingMap, reason: &str) {
    let drained: Vec<_> = match pending.lock() {
        Ok(mut map) => map.drain().collect(),
        Err(_) => return,
    };
    for (_, tx) in drained {
        let _ = tx.send(Err(browser_err(reason)));
    }
}

impl CdpSession {
    pub async fn connect(ws_url: &str) -> Result<Self> {
        let (stream, _) = tokio::time::timeout(Duration::from_secs(15), connect_async(ws_url))
            .await
            .map_err(|_| browser_err(format!("Timed out connecting to DevTools socket {ws_url}")))?
            .map_err(|e| browser_err(format!("DevTools socket connection failed: {e}")))?;
        let (sink, mut source) = stream.split();

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (events, _) = broadcast::channel(256);
        let closed = Arc::new(AtomicBool::new(false));

        let reader = {
            let pending = pending.clone();
            let events = events.clone();
            let closed = closed.clone();
            tokio::spawn(async move {
                while let Some(msg) = source.next().await {
                    let text = match msg {
                        Ok(Message::Text(t)) => t.to_string(),
                        Ok(Message::Binary(b)) => String::from_utf8_lossy(&b).into_owned(),
                        Ok(Message::Close(_)) | Err(_) => break,
                        Ok(_) => continue,
                    };
                    let Ok(val) = serde_json::from_str::<Value>(&text) else {
                        continue;
                    };
                    if let Some(id) = val.get("id").and_then(Value::as_u64) {
                        let tx = pending.lock().ok().and_then(|mut map| map.remove(&id));
                        if let Some(tx) = tx {
                            let res = match val.get("error") {
                                Some(err) => Err(browser_err(format!(
                                    "CDP error {}: {}",
                                    err.get("code").and_then(Value::as_i64).unwrap_or_default(),
                                    err.get("message").and_then(Value::as_str).unwrap_or("unknown")
                                ))),
                                None => Ok(val.get("result").cloned().unwrap_or(Value::Null)),
                            };
                            let _ = tx.send(res);
                        }
                    } else if let Some(method) = val.get("method").and_then(Value::as_str) {
                        let _ = events.send(CdpEvent {
                            method: method.to_string(),
                            params: val.get("params").cloned().unwrap_or(Value::Null),
                        });
                    }
                }
                closed.store(true, Ordering::SeqCst);
                fail_all(&pending, "DevTools connection closed");
                debug!("DevTools socket reader finished");
            })
        };

        Ok(Self {
            sink: tokio::sync::Mutex::new(sink),
            pending,
            next_id: AtomicU64::new(0),
            events,
            closed,
            reader,
        })
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CdpEvent> {
        self.events.subscribe()
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.call_with_timeout(method, params, DEFAULT_CALL_TIMEOUT).await
    }

    pub async fn call_with_timeout(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        if self.is_closed() {
            return Err(browser_err("DevTools connection closed"));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = oneshot::channel();
        if let Ok(mut map) = self.pending.lock() {
            map.insert(id, tx);
        }
        let payload = json!({ "id": id, "method": method, "params": params }).to_string();
        let sent = self.sink.lock().await.send(Message::Text(payload)).await;
        if let Err(e) = sent {
            self.forget(id);
            return Err(browser_err(format!("Failed to send {method}: {e}")));
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err(browser_err("DevTools connection closed")),
            Err(_) => {
                self.forget(id);
                Err(browser_err(format!("{method} timed out after {}s", timeout.as_secs())))
            }
        }
    }

    fn forget(&self, id: u64) {
        if let Ok(mut map) = self.pending.lock() {
            map.remove(&id);
        }
    }

    pub async fn close(&self) {
        let _ = self.sink.lock().await.close().await;
    }
}

impl Drop for CdpSession {
    fn drop(&mut self) {
        self.reader.abort();
        fail_all(&self.pending, "DevTools session dropped");
    }
}

#[derive(Debug, Clone)]
pub struct CdpTarget {
    pub id: String,
    pub ws_url: String,
}

fn parse_target(val: &Value) -> Option<CdpTarget> {
    Some(CdpTarget {
        id: val.get("id")?.as_str()?.to_string(),
        ws_url: val.get("webSocketDebuggerUrl")?.as_str()?.to_string(),
    })
}

pub fn devtools_http_base(endpoint: &str) -> String {
    let trimmed = endpoint.trim();
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    match reqwest::Url::parse(&with_scheme) {
        Ok(url) => {
            let scheme = match url.scheme() {
                "wss" | "https" => "https",
                _ => "http",
            };
            let host = url.host_str().unwrap_or("127.0.0.1");
            let host = if host.contains(':') { format!("[{host}]") } else { host.to_string() };
            match url.port() {
                Some(port) => format!("{scheme}://{host}:{port}"),
                None => format!("{scheme}://{host}"),
            }
        }
        Err(_) => with_scheme.trim_end_matches('/').to_string(),
    }
}

pub fn devtools_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub async fn open_tab(http: &reqwest::Client, http_base: &str, url: &str) -> Result<CdpTarget> {
    let endpoint = format!("{http_base}/json/new?{url}");
    let mut resp = http
        .put(&endpoint)
        .send()
        .await
        .map_err(|e| browser_err(format!("DevTools endpoint {http_base} unreachable: {e}")))?;
    if !resp.status().is_success() {
        resp = http
            .get(&endpoint)
            .send()
            .await
            .map_err(|e| browser_err(format!("DevTools endpoint {http_base} unreachable: {e}")))?;
    }
    if !resp.status().is_success() {
        return Err(browser_err(format!(
            "DevTools refused to open a tab (HTTP {})",
            resp.status().as_u16()
        )));
    }
    let val: Value = resp
        .json()
        .await
        .map_err(|e| browser_err(format!("Invalid DevTools /json/new response: {e}")))?;
    parse_target(&val).ok_or_else(|| browser_err("DevTools /json/new response missing target details"))
}

pub async fn find_tab(http: &reqwest::Client, http_base: &str, target_id: &str) -> Result<Option<CdpTarget>> {
    let resp = http
        .get(format!("{http_base}/json/list"))
        .send()
        .await
        .map_err(|e| browser_err(format!("DevTools endpoint {http_base} unreachable: {e}")))?;
    let list: Value = resp
        .json()
        .await
        .map_err(|e| browser_err(format!("Invalid DevTools /json/list response: {e}")))?;
    Ok(list.as_array().and_then(|items| {
        items
            .iter()
            .filter(|t| t.get("type").and_then(Value::as_str) == Some("page"))
            .filter(|t| t.get("id").and_then(Value::as_str) == Some(target_id))
            .find_map(parse_target)
    }))
}

pub async fn close_tab(http: &reqwest::Client, http_base: &str, target_id: &str) -> Result<()> {
    let endpoint = format!("{http_base}/json/close/{target_id}");
    let ok = match http.get(&endpoint).send().await {
        Ok(r) if r.status().is_success() => true,
        _ => matches!(http.put(&endpoint).send().await, Ok(r) if r.status().is_success()),
    };
    if ok {
        Ok(())
    } else {
        Err(browser_err(format!("DevTools could not close tab {target_id}")))
    }
}

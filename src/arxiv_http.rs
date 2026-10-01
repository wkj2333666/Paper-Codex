//! One application-wide lane for the legacy arXiv API. Never switch routes or
//! hide a 429 behind an alternate query; leave retries to the task scheduler.
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::{header::HeaderMap, Client};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::OnceLock, time::Duration};
use tokio::{sync::Mutex, time::Instant};
use url::Url;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Diagnostic {
    pub http_status: Option<u16>,
    pub error_kind: Option<String>,
    pub retry_after_seconds: Option<u64>,
    pub retry_at: Option<DateTime<Utc>>,
    pub server: Option<String>,
    pub cache_status: Option<String>,
    pub body_kind: Option<String>,
    pub cooldown_blocked: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ArxivError(pub Diagnostic);
impl std::fmt::Display for ArxivError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "arXiv {}",
            self.0.error_kind.as_deref().unwrap_or("request_failed")
        )?;
        if let Some(status) = self.0.http_status {
            write!(f, " (HTTP {status})")?;
        }
        if let Some(until) = self.0.retry_at {
            write!(
                f,
                "；冷却至 {}，期间不向 arXiv 发起新请求",
                until.to_rfc3339()
            )?;
        }
        Ok(())
    }
}
impl std::error::Error for ArxivError {}

#[derive(Default, Serialize, Deserialize)]
struct Cooldown {
    strikes: u32,
    diagnostic: Option<Diagnostic>,
}

#[derive(Default)]
struct State {
    next_start: Option<Instant>,
    cooldown: Cooldown,
    path: Option<PathBuf>,
}

struct Gate {
    state: Mutex<State>,
    spacing: Duration,
}

impl Gate {
    fn new(spacing: Duration) -> Self {
        Self {
            state: Mutex::new(State::default()),
            spacing,
        }
    }

    async fn initialize(&self, path: PathBuf) -> Result<()> {
        let mut state = self.state.lock().await;
        if state.path.is_some() {
            return Ok(());
        }
        match tokio::fs::read(&path).await {
            Ok(bytes) => {
                state.cooldown = serde_json::from_slice(&bytes).context("读取 arXiv 冷却记录")?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        state.path = Some(path);
        Ok(())
    }

    async fn request(
        &self,
        client: &Client,
        url: Url,
        timeout: Duration,
    ) -> Result<(String, Diagnostic)> {
        // Hold the lane through the response body, not just response headers.
        let mut state = self.state.lock().await;
        if let Some(mut diagnostic) = state
            .cooldown
            .diagnostic
            .clone()
            .filter(|d| d.retry_at.is_some_and(|until| until > Utc::now()))
        {
            diagnostic.cooldown_blocked = true;
            return Err(ArxivError(diagnostic).into());
        }
        if let Some(next) = state.next_start {
            tokio::time::sleep_until(next).await;
        }
        state.next_start = Some(Instant::now() + self.spacing);
        let mut response = client
            .get(url)
            .timeout(timeout)
            .send()
            .await
            .map_err(network_error)?;
        let now = Utc::now();
        let status = response.status();
        let retry = retry_after(response.headers(), now);
        let mut diagnostic = Diagnostic {
            http_status: Some(status.as_u16()),
            retry_after_seconds: retry,
            server: safe_header(response.headers(), "server"),
            cache_status: safe_header(response.headers(), "x-cache"),
            ..Default::default()
        };
        if !status.is_success() {
            diagnostic.error_kind = Some(
                if status.as_u16() == 429 {
                    "rate_limited"
                } else if status.is_server_error() {
                    "upstream_5xx"
                } else {
                    "http_rejected"
                }
                .into(),
            );
            if status.as_u16() == 429 || (status.is_server_error() && retry.is_some()) {
                state.cooldown.strikes = state.cooldown.strikes.saturating_add(1);
                let delay =
                    cooldown_seconds(state.cooldown.strikes, retry, rand::random_range(0..=60));
                diagnostic.retry_at = Some(
                    now.checked_add_signed(chrono::Duration::seconds(delay as i64))
                        .unwrap_or(DateTime::<Utc>::MAX_UTC),
                );
                state.cooldown.diagnostic = Some(diagnostic.clone());
                // Save before reading the error body: cancellation must not erase a 429.
                persist(&state).await?;
            }
            let mut prefix = Vec::new();
            let _ = tokio::time::timeout(Duration::from_secs(2), async {
                while prefix.len() < 2048 {
                    let Some(chunk) = response.chunk().await? else {
                        break;
                    };
                    prefix.extend_from_slice(&chunk[..chunk.len().min(2048 - prefix.len())]);
                }
                Ok::<(), reqwest::Error>(())
            })
            .await;
            diagnostic.body_kind = Some(body_kind(&prefix).into());
            if diagnostic.retry_at.is_some() {
                state.cooldown.diagnostic = Some(diagnostic.clone());
                persist(&state).await?;
            }
            tracing::warn!(status = status.as_u16(), kind = ?diagnostic.error_kind, retry_at = ?diagnostic.retry_at, server = ?diagnostic.server, cache = ?diagnostic.cache_status, body_kind = ?diagnostic.body_kind, "arxiv request rejected");
            return Err(ArxivError(diagnostic).into());
        }
        let body = response.text().await.map_err(network_error)?;
        if state.cooldown.strikes > 0 {
            state.cooldown = Cooldown::default();
            persist(&state).await?;
        }
        Ok((body, diagnostic))
    }
}

async fn persist(state: &State) -> Result<()> {
    if let Some(path) = &state.path {
        crate::workspace::atomic_write(path, &serde_json::to_vec(&state.cooldown)?).await?;
    }
    Ok(())
}

fn global() -> &'static Gate {
    static GATE: OnceLock<Gate> = OnceLock::new();
    GATE.get_or_init(|| Gate::new(Duration::from_secs(3)))
}

/// Called once before either interactive research or briefing workers start.
pub async fn initialize(path: PathBuf) -> Result<()> {
    global().initialize(path).await
}

pub(crate) async fn request(
    client: &Client,
    url: Url,
    timeout: Duration,
) -> Result<(String, Diagnostic)> {
    global().request(client, url, timeout).await
}

pub(crate) async fn cooling_until() -> Option<DateTime<Utc>> {
    global()
        .state
        .lock()
        .await
        .cooldown
        .diagnostic
        .as_ref()
        .and_then(|d| d.retry_at)
        .filter(|until| *until > Utc::now())
}

fn network_error(error: reqwest::Error) -> ArxivError {
    ArxivError(Diagnostic {
        error_kind: Some(
            if error.is_timeout() {
                "timeout"
            } else if error.is_connect() {
                "connection"
            } else {
                "network"
            }
            .into(),
        ),
        ..Default::default()
    })
}

fn safe_header(headers: &HeaderMap, name: &str) -> Option<String> {
    let value = headers.get(name)?.to_str().ok()?;
    // A deliberately small allowlist and alphabet; never persist cookies,
    // authorization, URLs, arbitrary error bodies or echoed user queries.
    (value.len() <= 160
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " ,.-_/()".contains(c)))
    .then(|| value.to_owned())
}

fn body_kind(body: &[u8]) -> &'static str {
    let text = String::from_utf8_lossy(body).to_ascii_lowercase();
    if text.contains("rate exceeded") {
        "rate_exceeded"
    } else if text.contains("too many requests") || text.contains("rate limit") {
        "rate_limit_notice"
    } else if text.contains("captcha") || text.contains("access denied") {
        "access_challenge"
    } else if text.contains("<html") {
        "html_error"
    } else if text.is_empty() {
        "empty"
    } else {
        "other_error"
    }
}

fn retry_after(headers: &HeaderMap, now: DateTime<Utc>) -> Option<u64> {
    let value = headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(seconds);
    }
    let until = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    // Honor server-relative time even if the local clock is a little skewed.
    let base = headers
        .get(reqwest::header::DATE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| DateTime::parse_from_rfc2822(v).ok())
        .map(|v| v.with_timezone(&Utc))
        .unwrap_or(now);
    Some((until - base).num_seconds().max(0) as u64)
}

fn cooldown_seconds(strikes: u32, server: Option<u64>, jitter: u64) -> u64 {
    let backoff = (15 * 60u64)
        .saturating_mul(1u64 << strikes.saturating_sub(1).min(3))
        .min(2 * 60 * 60);
    // Never clamp the server's requested delay down to our local maximum.
    backoff
        .max(server.unwrap_or(0))
        .saturating_add(jitter)
        .min(chrono::Duration::MAX.num_seconds() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn respects_seconds_dates_clock_skew_and_long_server_cooldowns() {
        let now = DateTime::parse_from_rfc3339("2026-10-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "72000".parse().unwrap());
        assert_eq!(retry_after(&headers, now), Some(72000));
        assert_eq!(cooldown_seconds(1, retry_after(&headers, now), 0), 72000);
        headers.insert("date", "Thu, 01 Oct 2026 01:00:00 GMT".parse().unwrap());
        headers.insert(
            "retry-after",
            "Thu, 01 Oct 2026 01:30:00 GMT".parse().unwrap(),
        );
        assert_eq!(retry_after(&headers, now), Some(1800));
        headers.insert("retry-after", "invalid".parse().unwrap());
        assert_eq!(retry_after(&headers, now), None);
        assert_eq!(cooldown_seconds(1, None, 0), 900);
        assert_eq!(cooldown_seconds(2, Some(0), 0), 1800);
        assert_eq!(cooldown_seconds(99, None, 0), 7200);
    }

    #[tokio::test]
    async fn rate_limit_blocks_other_queries_and_survives_restart_without_leaking_body() {
        let hits = Arc::new(AtomicUsize::new(0));
        let count = hits.clone();
        let app = Router::new().route(
            "/",
            get(move || {
                let count = count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    (
                        axum::http::StatusCode::TOO_MANY_REQUESTS,
                        [("retry-after", "72000"), ("server", "Google Frontend")],
                        "Rate exceeded. private-query secret-token",
                    )
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url: Url = format!("http://{}/", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cooldown.json");
        let gate = Gate::new(Duration::from_millis(5));
        gate.initialize(path.clone()).await.unwrap();
        let client = Client::builder().no_proxy().build().unwrap();
        let first = gate
            .request(&client, url.clone(), Duration::from_secs(2))
            .await
            .unwrap_err();
        let error = first.downcast_ref::<ArxivError>().unwrap();
        assert_eq!(error.0.http_status, Some(429));
        assert_eq!(error.0.body_kind.as_deref(), Some("rate_exceeded"));
        assert!(!error.0.cooldown_blocked);
        let restarted = Gate::new(Duration::from_millis(5));
        restarted.initialize(path.clone()).await.unwrap();
        let second = restarted
            .request(
                &client,
                url.join("?different-query").unwrap(),
                Duration::from_secs(2),
            )
            .await
            .unwrap_err();
        assert!(
            second
                .downcast_ref::<ArxivError>()
                .unwrap()
                .0
                .cooldown_blocked
        );
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        let saved = tokio::fs::read_to_string(path).await.unwrap();
        assert!(!saved.contains("private-query"));
        assert!(!saved.contains("secret-token"));
        server.abort();
    }

    #[tokio::test]
    async fn concurrent_callers_use_one_spaced_lane() {
        let arrivals = Arc::new(Mutex::new(Vec::new()));
        let observed = arrivals.clone();
        let app = Router::new().route(
            "/",
            get(move || {
                let observed = observed.clone();
                async move {
                    observed.lock().await.push(Instant::now());
                    axum::body::Body::from_stream(async_stream::stream! {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        yield Ok::<_,std::convert::Infallible>("ok");
                    })
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url: Url = format!("http://{}/", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let gate = Gate::new(Duration::from_millis(50));
        let client = Client::builder().no_proxy().build().unwrap();
        let (a, b) = tokio::join!(
            gate.request(&client, url.clone(), Duration::from_secs(2)),
            gate.request(&client, url, Duration::from_secs(2))
        );
        a.unwrap();
        b.unwrap();
        let times = arrivals.lock().await;
        assert_eq!(times.len(), 2);
        // The 100 ms response body, not merely the 50 ms spacing, holds the lane.
        assert!(times[1].duration_since(times[0]) >= Duration::from_millis(90));
        server.abort();
    }
}

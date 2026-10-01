//! Auditable retrieval; incomplete feeds must never become a successful empty day.
use crate::research::WorkMetadata;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// Only validated pages from this exact issue/query/window are reusable. Do not
// cache errors, empty malformed feeds, or pretend old pages were fetched now.
#[derive(Serialize, Deserialize)]
pub(crate) struct CachedPage {
    pub fetched_at: DateTime<Utc>,
    pub entries: Vec<WorkMetadata>,
    pub total: Option<usize>,
    pub diagnostic: crate::arxiv_http::Diagnostic,
}
pub(crate) fn cache_path(
    workspace: &crate::workspace::Workspace,
    id: &str,
    query: &str,
    start: usize,
    since: DateTime<Utc>,
) -> std::path::PathBuf {
    let key = hex::encode(Sha256::digest(format!(
        "arxiv-page-v1:{id}:{query}:{start}:{since}:200"
    )));
    workspace
        .state_dir()
        .join("briefing-fetch")
        .join(format!("{key}.json"))
}
pub(crate) async fn cached_page(path: &std::path::Path, now: DateTime<Utc>) -> Option<CachedPage> {
    let page: CachedPage = serde_json::from_slice(&tokio::fs::read(path).await.ok()?).ok()?;
    let age = now.signed_duration_since(page.fetched_at);
    (age >= Duration::zero() && age < Duration::minutes(90)).then_some(page)
}

pub(crate) fn issue_start(settings: &serde_json::Value, started_at: &str) -> DateTime<Utc> {
    // Freeze a first-ever project's window across retries as well; otherwise
    // `since(None, now)` changes every attempt and defeats exact-page caching.
    settings["attempt_history"][0]["started_at"]
        .as_str()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .or_else(|| DateTime::parse_from_rfc3339(started_at).ok())
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or_else(Utc::now)
}

pub(crate) async fn prune_cache(workspace: &crate::workspace::Workspace) -> Result<()> {
    let directory = workspace.state_dir().join("briefing-fetch");
    let mut entries = match tokio::fs::read_dir(directory).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(hash) = name.strip_suffix(".json") else {
            continue;
        };
        if hash.len() != 64
            || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !entry.file_type().await?.is_file()
        {
            continue;
        }
        let expired = entry
            .metadata()
            .await?
            .modified()?
            .elapsed()
            .is_ok_and(|age| age > std::time::Duration::from_secs(7 * 24 * 60 * 60));
        if expired {
            tokio::fs::remove_file(entry.path()).await?;
        }
    }
    Ok(())
}

pub(crate) const REQUEST_TIMEOUT_SECONDS: u64 = 60;

pub(crate) fn fallback_eligible(error: &anyhow::Error) -> bool {
    if let Some(error) = error.downcast_ref::<crate::arxiv_http::ArxivError>() {
        return error.0.retry_at.is_none()
            && matches!(
                error.0.error_kind.as_deref(),
                Some("timeout" | "connection" | "upstream_5xx")
            );
    }
    error.downcast_ref::<reqwest::Error>().is_some_and(|error| {
        error.is_timeout()
            || error.is_connect()
            || error
                .status()
                .is_some_and(|status| status.is_server_error())
    })
}

pub(crate) fn error_kind(error: &anyhow::Error) -> &'static str {
    if let Some(error) = error.downcast_ref::<crate::arxiv_http::ArxivError>() {
        return match error.0.error_kind.as_deref() {
            Some("rate_limited") => "rate_limited",
            Some("timeout") => "timeout",
            Some("connection") => "connection",
            Some("upstream_5xx") => "upstream_5xx",
            _ => "http_rejected",
        };
    }
    match error.downcast_ref::<reqwest::Error>() {
        Some(error) if error.is_timeout() => "timeout",
        Some(error) if error.is_connect() => "connection",
        Some(error)
            if error
                .status()
                .is_some_and(|status| status.is_server_error()) =>
        {
            "upstream_5xx"
        }
        Some(error) if error.status().is_some() => "http_rejected",
        _ => "invalid_response",
    }
}

pub(crate) fn since(previous: Option<&str>, now: DateTime<Utc>) -> DateTime<Utc> {
    let previous = previous
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or(now)
        .min(now);
    (previous - Duration::days(7)).max(now - Duration::days(30))
}

pub(crate) fn total_results(body: &str) -> Result<Option<usize>> {
    #[derive(Deserialize)]
    struct Envelope {
        #[serde(rename = "totalResults", default)]
        total: Option<usize>,
    }
    Ok(quick_xml::de::from_str::<Envelope>(body)
        .context("读取 arXiv 结果总数失败")?
        .total)
}

#[derive(Default, Serialize)]
pub(crate) struct FetchAudit {
    pub query: String,
    pub pages: usize,
    pub total_results: Option<usize>,
    pub received: usize,
    pub within_window: usize,
    pub before_window: usize,
    pub latest_updated: Option<String>,
    pub oldest_updated: Option<String>,
    pub complete: bool,
    pub limit_reached: bool,
    pub request_timeout_seconds: u64,
    pub last_request_ms: u64,
    pub error_kind: Option<String>,
    pub response: Option<crate::arxiv_http::Diagnostic>,
    pub cached_pages: usize,
    pub last_fetched_at: Option<DateTime<Utc>>,
    #[serde(skip)]
    last_date: Option<DateTime<Utc>>,
}
impl FetchAudit {
    pub fn new(query: &str) -> Self {
        Self {
            query: query.into(),
            request_timeout_seconds: REQUEST_TIMEOUT_SECONDS,
            ..Default::default()
        }
    }
    pub fn page(
        &mut self,
        entries: Vec<WorkMetadata>,
        total: Option<usize>,
        since: DateTime<Utc>,
    ) -> Result<Vec<WorkMetadata>> {
        self.pages += 1;
        self.total_results = total;
        if entries.is_empty()
            && (total.is_none() || total.is_some_and(|count| count > self.received))
        {
            bail!("arXiv 返回不完整的空结果页，不能判定为无新增论文");
        }
        if total == Some(0) && !entries.is_empty() {
            bail!("arXiv 总数与结果页不一致");
        }
        let count = entries.len();
        self.received += count;
        let mut recent = Vec::new();
        for paper in entries {
            let updated = paper.metadata["updated"]
                .as_str()
                .context("arXiv 返回缺少更新时间")?;
            let date = DateTime::parse_from_rfc3339(updated)?.with_timezone(&Utc);
            if self.last_date.is_some_and(|previous| date > previous) {
                bail!("arXiv 结果未按更新时间排序，停止推进检索位置");
            }
            self.last_date = Some(date);
            if self.latest_updated.is_none() {
                self.latest_updated = Some(updated.to_owned());
            }
            self.oldest_updated = Some(updated.to_owned());
            if date < since {
                self.before_window += 1;
            } else {
                self.within_window += 1;
                recent.push(paper);
            }
        }
        self.complete = self.before_window > 0
            || total
                .map(|count| self.received >= count)
                .unwrap_or(count < 200);
        Ok(recent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retries_keep_the_original_issue_window() {
        let history =
            serde_json::json!({"attempt_history":[{"started_at":"2026-10-01T00:00:00Z"}]});
        let first = issue_start(&serde_json::json!({}), "2026-10-01T00:00:00Z");
        assert_eq!(first, issue_start(&history, "2026-10-01T01:00:00Z"));
        assert_eq!(
            since(None, first),
            since(None, issue_start(&history, "2026-10-01T01:00:00Z"))
        );
    }
    #[tokio::test]
    async fn cache_cleanup_preserves_recent_pages_and_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = crate::workspace::Workspace::initialize(dir.path())
            .await
            .unwrap();
        let cache = workspace.state_dir().join("briefing-fetch");
        let old = cache.join(format!("{}.json", "a".repeat(64)));
        let recent = cache.join(format!("{}.json", "b".repeat(64)));
        let user_file = cache.join("notes.json");
        for path in [&old, &recent, &user_file] {
            crate::workspace::atomic_write(path, b"{}").await.unwrap();
        }
        for path in [&old, &user_file] {
            std::fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(
                    std::time::SystemTime::now() - std::time::Duration::from_secs(8 * 24 * 60 * 60),
                ))
                .unwrap();
        }
        prune_cache(&workspace).await.unwrap();
        assert!(!old.exists());
        assert!(recent.exists());
        assert!(user_file.exists());
    }
    #[tokio::test]
    async fn validated_page_cache_is_issue_query_window_specific_and_expires() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = crate::workspace::Workspace::initialize(dir.path())
            .await
            .unwrap();
        let now = Utc::now();
        let path = cache_path(&workspace, "issue", "cat:cs.RO", 0, now);
        assert_ne!(path, cache_path(&workspace, "other", "cat:cs.RO", 0, now));
        assert_ne!(path, cache_path(&workspace, "issue", "cat:cs.AI", 0, now));
        assert_ne!(path, cache_path(&workspace, "issue", "cat:cs.RO", 200, now));
        assert_ne!(
            path,
            cache_path(&workspace, "issue", "cat:cs.RO", 0, now - Duration::days(1))
        );
        let page = CachedPage {
            fetched_at: now,
            entries: vec![],
            total: Some(0),
            diagnostic: Default::default(),
        };
        crate::workspace::atomic_write(&path, &serde_json::to_vec(&page).unwrap())
            .await
            .unwrap();
        assert!(cached_page(&path, now + Duration::minutes(30))
            .await
            .is_some());
        assert!(cached_page(&path, now + Duration::minutes(91))
            .await
            .is_none());
        assert!(cached_page(&path, now - Duration::seconds(1))
            .await
            .is_none());
        crate::workspace::atomic_write(&path, b"invalid")
            .await
            .unwrap();
        assert!(cached_page(&path, now).await.is_none());
    }
    #[test]
    fn cooldown_must_not_trigger_an_alternate_query() {
        let error: anyhow::Error = crate::arxiv_http::ArxivError(crate::arxiv_http::Diagnostic {
            http_status: Some(429),
            error_kind: Some("rate_limited".into()),
            retry_at: Some(Utc::now() + Duration::minutes(15)),
            ..Default::default()
        })
        .into();
        assert!(!fallback_eligible(&error));
        assert_eq!(error_kind(&error), "rate_limited");
        let error: anyhow::Error = crate::arxiv_http::ArxivError(crate::arxiv_http::Diagnostic {
            http_status: Some(503),
            error_kind: Some("upstream_5xx".into()),
            retry_at: Some(Utc::now() + Duration::minutes(15)),
            ..Default::default()
        })
        .into();
        assert!(!fallback_eligible(&error));
    }
    #[test]
    fn server_errors_can_fallback_but_rejections_and_invalid_data_cannot() {
        for (status, expected) in [
            (503, true),
            (504, true),
            (429, false),
            (401, false),
            (403, false),
            (400, false),
        ] {
            let response = reqwest::Response::from(
                axum::http::Response::builder()
                    .status(status)
                    .body("")
                    .unwrap(),
            );
            let error: anyhow::Error = response.error_for_status().unwrap_err().into();
            assert_eq!(fallback_eligible(&error), expected);
        }
        assert!(!fallback_eligible(&anyhow::anyhow!("invalid feed")));
        assert_eq!(
            error_kind(&anyhow::anyhow!("invalid feed")),
            "invalid_response"
        );
        assert_eq!(FetchAudit::new("query").request_timeout_seconds, 60);
    }

    #[tokio::test]
    async fn request_timeout_is_auditable_and_can_trigger_one_fallback() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });
        let error: anyhow::Error = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://{address}/"))
            .timeout(std::time::Duration::from_millis(30))
            .send()
            .await
            .unwrap_err()
            .into();
        assert!(fallback_eligible(&error));
        assert_eq!(error_kind(&error), "timeout");
        server.abort();
    }
    #[test]
    fn parses_prefixed_total_and_rejects_false_empty_pages() {
        assert_eq!(total_results(r#"<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opensearch="http://a9.com/-/spec/opensearch/1.1/"><opensearch:totalResults>42</opensearch:totalResults></feed>"#).unwrap(), Some(42));
        let mut audit = FetchAudit::new("query");
        assert!(audit.page(vec![], Some(42), Utc::now()).is_err());
        assert!(!audit.complete);
        let mut audit = FetchAudit::new("query");
        assert!(audit.page(vec![], None, Utc::now()).is_err());
        let mut audit = FetchAudit::new("query");
        assert!(audit.page(vec![], Some(0), Utc::now()).unwrap().is_empty());
        assert!(audit.complete);
    }
    #[test]
    fn overlap_covers_weekends_and_delayed_indexing_but_is_bounded() {
        let now = DateTime::parse_from_rfc3339("2026-09-28T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            since(Some("2026-09-27T08:49:38Z"), now).to_rfc3339(),
            "2026-09-20T08:49:38+00:00"
        );
        assert_eq!(since(None, now), now - Duration::days(7));
        assert_eq!(
            since(Some("2020-01-01T00:00:00Z"), now),
            now - Duration::days(30)
        );
    }

    #[test]
    fn counts_date_filter_and_does_not_finish_a_short_partial_page() {
        let mut works = crate::research_providers::parse_arxiv_search(include_str!(
            "../fixtures/research/arxiv-search.xml"
        ))
        .unwrap();
        works[0].metadata["updated"] = serde_json::json!("2026-09-25T14:21:20Z");
        works[1].metadata["updated"] = serde_json::json!("2026-09-24T00:00:00Z");
        let since = DateTime::parse_from_rfc3339("2026-09-25T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut audit = FetchAudit::new("query");
        assert_eq!(
            audit
                .page(vec![works[0].clone()], Some(200), since)
                .unwrap()
                .len(),
            1
        );
        assert!(!audit.complete);
        assert_eq!(audit.received, 1);
        assert_eq!(
            audit
                .page(vec![works[1].clone()], Some(200), since)
                .unwrap()
                .len(),
            0
        );
        assert!(audit.complete);
        assert_eq!(audit.within_window, 1);
        assert_eq!(audit.before_window, 1);
        let mut audit = FetchAudit::new("query");
        assert!(audit
            .page(vec![works[1].clone(), works[0].clone()], Some(2), since)
            .is_err());
        assert!(!audit.complete);
    }
}

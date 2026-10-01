//! Persistent daily briefings. Scheduling and delivery do not depend on a browser.
use crate::{
    codex::{CodexRuntime, CodexTurn},
    conversation_engine::ConversationEngine,
    conversations::ConversationScopeInput,
    db::Database,
    research::WorkMetadata,
    research_providers::{parse_arxiv_search, research_http_client},
    research_service::ResearchService,
    workspace::{atomic_write, Workspace},
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, FixedOffset, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::AsyncWriteExt,
    sync::{watch, Mutex},
};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS daily_briefings (
 id TEXT PRIMARY KEY, project_id TEXT NOT NULL DEFAULT '', day TEXT NOT NULL, status TEXT NOT NULL,
 markdown TEXT NOT NULL DEFAULT '', error TEXT, conversation_id TEXT,
 mail_status TEXT NOT NULL DEFAULT 'pending', mail_attempts INTEGER NOT NULL DEFAULT 0,
 attempts INTEGER NOT NULL DEFAULT 0, started_at TEXT NOT NULL, completed_at TEXT,
 sources_json TEXT NOT NULL DEFAULT '[]', settings_json TEXT NOT NULL,
 next_attempt_at TEXT, mail_next_attempt_at TEXT, mail_error TEXT, UNIQUE(project_id, day)
);
CREATE TABLE IF NOT EXISTS briefing_seen (project_id TEXT NOT NULL, paper_id TEXT NOT NULL, updated TEXT NOT NULL,
 PRIMARY KEY(project_id, paper_id, updated));
CREATE TABLE IF NOT EXISTS briefing_legacy_owner (singleton INTEGER PRIMARY KEY CHECK(singleton=1), project_id TEXT NOT NULL);
"#;

#[derive(Debug, thiserror::Error)]
#[error("模型拒绝或无法完成晨报，请检查模型提供商/凭据后手动重试；未进行自动重试")]
struct ModelRejected;

fn permanent_failure(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ModelRejected>().is_some()
        || error
            .downcast_ref::<crate::arxiv_http::ArxivError>()
            .is_some_and(|error| {
                error
                    .0
                    .http_status
                    .is_some_and(|status| (400..500).contains(&status) && status != 429)
            })
        || error
            .downcast_ref::<reqwest::Error>()
            .and_then(|e| e.status())
            .is_some_and(|status| status.is_client_error() && status.as_u16() != 429)
}

fn retry_time(error: &anyhow::Error, attempt: i64, now: DateTime<Utc>) -> DateTime<Utc> {
    let base = now + chrono::Duration::minutes(10 * (1i64 << attempt.saturating_sub(1).min(3)));
    error
        .downcast_ref::<crate::arxiv_http::ArxivError>()
        .and_then(|error| error.0.retry_at)
        .map_or(base, |until| base.max(until))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BriefingConfig {
    pub project_id: String,
    pub enabled: bool,
    pub time: String,
    pub timezone: String,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    #[serde(skip_serializing)]
    pub project_ids: Vec<String>, // Read-only compatibility with the old YAML.
    pub max_papers: usize,
    pub fulltext_papers: usize,
    pub timeout_minutes: u64,
    pub email_enabled: bool,
    pub recipient: String,
}
impl Default for BriefingConfig {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            enabled: false,
            time: "09:30".into(),
            timezone: "Asia/Shanghai".into(),
            categories: vec!["cs.RO".into(), "cs.CV".into(), "cs.AI".into()],
            keywords: vec![],
            project_ids: vec![],
            max_papers: 12,
            fulltext_papers: 4,
            timeout_minutes: 15,
            email_enabled: false,
            recipient: String::new(),
        }
    }
}
impl BriefingConfig {
    fn normalize(mut self) -> Self {
        if self.project_id.is_empty() && self.project_ids.len() == 1 {
            self.project_id = self.project_ids[0].clone();
        }
        self.project_ids.clear();
        self
    }
    pub fn validate(&self) -> Result<()> {
        if self.project_id.trim().is_empty() {
            bail!("每份晨报必须选择一个所属项目");
        }
        NaiveTime::parse_from_str(&self.time, "%H:%M").context("时间须为 HH:MM")?;
        if self.timezone != "Asia/Shanghai" {
            bail!("目前晨报使用 Asia/Shanghai 时区");
        }
        if !(1..=20).contains(&self.max_papers)
            || self.fulltext_papers > 5
            || self.fulltext_papers > self.max_papers
            || !(2..=30).contains(&self.timeout_minutes)
        {
            bail!("篇数须为 1–20，全文最多 5 篇，超时须为 2–30 分钟");
        }
        if self.categories.is_empty()
            || self.categories.len() > 5
            || self.categories.iter().any(|v| {
                v.is_empty()
                    || !v
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
            })
        {
            bail!("请填写有效 arXiv 分类，最多 5 个");
        }
        if self.keywords.len() > 40
            || self
                .keywords
                .iter()
                .any(|v| v.trim().is_empty() || v.len() > 100)
        {
            bail!("补充关注词最多 40 个，每个须为 1–100 字节");
        }
        if self.email_enabled && !valid_mailbox(&self.recipient) {
            bail!("请填写有效收件邮箱");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BriefingSettings {
    pub projects: Vec<BriefingConfig>,
}
impl BriefingSettings {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let value: serde_yaml::Value = serde_yaml::from_slice(bytes)?;
        let mut settings = if value.get("projects").is_some() {
            serde_yaml::from_value::<Self>(value)?
        } else {
            Self {
                projects: vec![serde_yaml::from_value::<BriefingConfig>(value)?],
            }
        };
        if settings.projects.len() > 20 {
            bail!("最多配置 20 个项目晨报");
        }
        let mut seen = std::collections::BTreeSet::new();
        for config in &mut settings.projects {
            *config = config.clone().normalize();
            config.validate()?;
            if !seen.insert(config.project_id.clone()) {
                bail!("同一项目不能配置重复晨报");
            }
        }
        Ok(settings)
    }
}
fn valid_mailbox(value: &str) -> bool {
    value.len() <= 254
        && value.split('@').count() == 2
        && value.split('@').all(|v| !v.is_empty())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "@._+-".contains(c))
}

pub(crate) fn selected_works(markdown: &str, sources: &[Value]) -> Vec<WorkMetadata> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    static BARE_URL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let bare_url =
        BARE_URL.get_or_init(|| regex::Regex::new(r#"https?://[^\s<>\[\]()\"']+"#).unwrap());
    let mut links = std::collections::BTreeSet::new();
    let mut in_code = false;
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::CodeBlock(_)) => in_code = true,
            Event::End(TagEnd::CodeBlock) => in_code = false,
            Event::Start(Tag::Link { dest_url, .. }) => {
                links.insert(dest_url.to_string());
            }
            // Earlier reports used plain URLs, which Markdown renders as text.
            // Match complete URLs, not substrings (paper 123 must not match 1234).
            Event::Text(text) if !in_code => {
                for matched in bare_url.find_iter(&text) {
                    links.insert(
                        matched
                            .as_str()
                            .trim_end_matches(['.', ',', ';', '。', '，', '；', '）'])
                            .to_owned(),
                    );
                }
            }
            _ => {}
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    sources
        .iter()
        .filter_map(|source| serde_json::from_value::<WorkMetadata>(source["paper"].clone()).ok())
        .filter(|paper| {
            links.contains(&paper.source_url) && seen.insert(paper.canonical_key.clone())
        })
        .collect()
}
pub fn local_day(now: DateTime<Utc>) -> String {
    now.with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap())
        .format("%Y-%m-%d")
        .to_string()
}
pub fn due(config: &BriefingConfig, now: DateTime<Utc>) -> bool {
    config.enabled
        && NaiveTime::parse_from_str(&config.time, "%H:%M").is_ok_and(|time| {
            now.with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap())
                .time()
                >= time
        })
}

#[derive(Clone, Serialize, sqlx::FromRow)]
pub struct Briefing {
    pub id: String,
    pub project_id: String,
    pub day: String,
    pub status: String,
    pub markdown: String,
    pub error: Option<String>,
    pub conversation_id: Option<String>,
    pub mail_status: String,
    pub mail_attempts: i64,
    pub attempts: i64,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub sources_json: String,
    pub settings_json: String,
    pub next_attempt_at: Option<String>,
    pub mail_next_attempt_at: Option<String>,
    pub mail_error: Option<String>,
}

pub struct BriefingService {
    db: Database,
    workspace: Workspace,
    codex: Arc<CodexRuntime>,
    conversations: Arc<ConversationEngine>,
    research: Arc<ResearchService>,
    config_path: PathBuf,
    mail_env_path: Option<PathBuf>,
    gate: Mutex<()>,
    config_gate: Mutex<()>,
}
impl BriefingService {
    pub async fn recover_states(db: &Database) -> Result<()> {
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('daily_briefings')")
                .fetch_all(db.pool())
                .await?;
        let legacy = !columns.is_empty() && !columns.iter().any(|name| name == "project_id");
        let mut tx = db.pool().begin_with("BEGIN IMMEDIATE").await?;
        if legacy {
            // Preserve the original tables as a rollback/audit copy. No model,
            // filesystem or network work is performed while this lock is held.
            sqlx::raw_sql("ALTER TABLE daily_briefings RENAME TO daily_briefings_legacy_v1; ALTER TABLE briefing_seen RENAME TO briefing_seen_legacy_v1;").execute(&mut *tx).await?;
        }
        sqlx::raw_sql(SCHEMA).execute(&mut *tx).await?;
        let mail_column: i64 = sqlx::query_scalar("SELECT count(*) FROM pragma_table_info('daily_briefings') WHERE name='mail_next_attempt_at'").fetch_one(&mut *tx).await?;
        if mail_column == 0 {
            sqlx::query("ALTER TABLE daily_briefings ADD COLUMN mail_next_attempt_at TEXT")
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE daily_briefings SET mail_next_attempt_at=next_attempt_at WHERE status IN ('completed','empty')").execute(&mut *tx).await?;
        }
        if legacy {
            sqlx::query("INSERT INTO daily_briefings(id,project_id,day,status,markdown,error,conversation_id,mail_status,mail_attempts,attempts,started_at,completed_at,sources_json,settings_json,next_attempt_at,mail_error) SELECT id,'',day,status,markdown,error,conversation_id,mail_status,mail_attempts,attempts,started_at,completed_at,sources_json,settings_json,next_attempt_at,mail_error FROM daily_briefings_legacy_v1").execute(&mut *tx).await?;
        }
        tx.commit().await?;
        sqlx::query("UPDATE daily_briefings SET status='failed',error='服务重启中断生成',next_attempt_at=? WHERE status='running'").bind((Utc::now() + chrono::Duration::minutes(10)).to_rfc3339()).execute(db.pool()).await?;
        // SMTP acceptance may have happened before a crash: do not send blindly again.
        sqlx::query("UPDATE daily_briefings SET mail_status='uncertain',mail_error='发送时服务中断，请检查邮箱后手动重发' WHERE mail_status='sending'").execute(db.pool()).await?;
        Ok(())
    }
    pub async fn start(
        db: Database,
        workspace: Workspace,
        codex: Arc<CodexRuntime>,
        conversations: Arc<ConversationEngine>,
        research: Arc<ResearchService>,
        config_path: PathBuf,
        mail_env_path: Option<PathBuf>,
    ) -> Result<Arc<Self>> {
        if !config_path.exists() {
            atomic_write(
                &config_path,
                serde_yaml::to_string(&BriefingSettings::default())?.as_bytes(),
            )
            .await?;
        }
        let service = Arc::new(Self {
            db,
            workspace,
            codex,
            conversations,
            research,
            config_path,
            mail_env_path,
            gate: Mutex::new(()),
            config_gate: Mutex::new(()),
        });
        if let Ok(settings) = service.settings().await {
            if settings.projects.len() == 1 {
                Self::adopt_legacy(&service.db, &settings.projects[0].project_id).await?;
            }
        }
        let worker = service.clone();
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(Duration::from_secs(60));
            loop {
                timer.tick().await;
                if let Err(error) = worker.tick().await {
                    tracing::warn!(error = %error, "daily briefing scheduler");
                }
            }
        });
        Ok(service)
    }
    pub async fn settings(&self) -> Result<BriefingSettings> {
        let bytes = tokio::fs::read(&self.config_path)
            .await
            .context("读取晨报配置失败")?;
        BriefingSettings::parse(&bytes).context("晨报 YAML 配置无效；旧晨报须先指定唯一所属项目")
    }
    pub async fn config(&self, project_id: &str) -> Result<BriefingConfig> {
        self.settings()
            .await?
            .projects
            .into_iter()
            .find(|config| config.project_id == project_id)
            .context("该项目尚未配置晨报")
    }
    pub async fn adopt_legacy(db: &Database, project_id: &str) -> Result<()> {
        db.get_project(project_id)
            .await?
            .context("晨报项目不存在")?;
        let exists: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='briefing_seen_legacy_v1'").fetch_one(db.pool()).await?;
        if exists == 0 {
            return Ok(());
        }
        let mut tx = db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let claimed = sqlx::query(
            "INSERT OR IGNORE INTO briefing_legacy_owner(singleton,project_id) VALUES(1,?)",
        )
        .bind(project_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if claimed > 0 {
            sqlx::query("UPDATE daily_briefings SET project_id=? WHERE project_id=''")
                .bind(project_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT OR IGNORE INTO briefing_seen(project_id,paper_id,updated) SELECT ?,paper_id,updated FROM briefing_seen_legacy_v1").bind(project_id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
    pub async fn save_config(&self, config: BriefingConfig) -> Result<()> {
        let config = config.normalize();
        config.validate()?;
        self.db
            .get_project(&config.project_id)
            .await?
            .context("晨报项目不存在")?;
        if config.email_enabled && !self.mail_configured() {
            bail!("尚未配置本地发信凭据文件");
        }
        let _guard = self.config_gate.lock().await;
        let mut settings = self.settings().await?;
        settings
            .projects
            .retain(|current| current.project_id != config.project_id);
        settings.projects.push(config);
        if settings.projects.len() > 20 {
            bail!("最多配置 20 个项目晨报");
        }
        atomic_write(
            &self.config_path,
            serde_yaml::to_string(&settings)?.as_bytes(),
        )
        .await
    }
    pub async fn list(&self) -> Result<Vec<Briefing>> {
        // Poll only small status records. Fetch the chosen day's body separately.
        Ok(sqlx::query_as("SELECT id,project_id,day,status,'' AS markdown,error,conversation_id,mail_status,mail_attempts,attempts,started_at,completed_at,'[]' AS sources_json,'{}' AS settings_json,next_attempt_at,mail_next_attempt_at,mail_error FROM daily_briefings ORDER BY day DESC,started_at DESC LIMIT 300").fetch_all(self.db.pool()).await?)
    }
    pub async fn get(&self, id: &str) -> Result<Briefing> {
        sqlx::query_as("SELECT * FROM daily_briefings WHERE id=?")
            .bind(id)
            .fetch_optional(self.db.pool())
            .await?
            .context("晨报不存在")
    }
    pub fn mail_configured(&self) -> bool {
        self.mail_env_path.as_ref().is_some_and(|p| p.is_file())
    }
    async fn tick(self: &Arc<Self>) -> Result<()> {
        for config in self.settings().await?.projects {
            if self.db.get_project(&config.project_id).await?.is_none() {
                continue;
            }
            if due(&config, Utc::now()) {
                let busy: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM daily_briefings WHERE status='running'",
                )
                .fetch_one(self.db.pool())
                .await?;
                if busy == 0 {
                    if let Err(error) = self.launch(&config.project_id, false).await {
                        tracing::warn!(project_id = %config.project_id, %error, "project briefing launch failed");
                    }
                }
            }
            if config.email_enabled {
                let items: Vec<String> = sqlx::query_scalar("SELECT id FROM daily_briefings WHERE project_id=? AND (status IN ('completed','empty') OR (status='failed' AND (attempts>=3 OR next_attempt_at IS NULL))) AND mail_status IN ('pending','failed') AND mail_attempts<3 AND (mail_next_attempt_at IS NULL OR mail_next_attempt_at<=?) ORDER BY day LIMIT 1")
                    .bind(&config.project_id).bind(Utc::now().to_rfc3339()).fetch_all(self.db.pool()).await?;
                for id in items {
                    if let Err(error) = self.deliver(&id, false, None).await {
                        tracing::warn!(project_id = %config.project_id, %error, "project briefing delivery failed");
                    }
                }
            }
        }
        Ok(())
    }
    pub async fn launch(self: &Arc<Self>, project_id: &str, manual: bool) -> Result<String> {
        let config = self.config(project_id).await?;
        self.db
            .get_project(project_id)
            .await?
            .context("晨报项目已被删除")?;
        let _guard = self.gate.lock().await;
        let day = local_day(Utc::now());
        let old: Option<Briefing> =
            sqlx::query_as("SELECT * FROM daily_briefings WHERE project_id=? AND day=?")
                .bind(project_id)
                .bind(&day)
                .fetch_optional(self.db.pool())
                .await?;
        if let Some(ref item) = old {
            if !retry_allowed(item, manual, &Utc::now().to_rfc3339()) {
                return Ok(item.id.clone());
            }
            if matches!(item.mail_status.as_str(), "sending" | "uncertain") {
                bail!("请先确认上次邮件的投递结果，再重新生成");
            }
        }
        if let Some(until) = crate::arxiv_http::cooling_until().await {
            bail!(
                "arXiv 正在冷却至 {}；未发起检索，也未增加尝试次数",
                until.to_rfc3339()
            );
        }
        if !manual
            && old
                .as_ref()
                .is_some_and(|item| item.next_attempt_at.is_none())
        {
            return Ok(old.as_ref().unwrap().id.clone());
        }
        let busy: i64 =
            sqlx::query_scalar("SELECT count(*) FROM daily_briefings WHERE status='running'")
                .fetch_one(self.db.pool())
                .await?;
        if busy > 0 {
            bail!("已有晨报正在生成");
        }
        let settings = attempt_settings(&config, old.as_ref())?;
        let attempt = old.as_ref().map_or(1, |item| item.attempts + 1);
        let id = old
            .map(|v| v.id)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        sqlx::query("INSERT INTO daily_briefings(id,project_id,day,status,started_at,settings_json,attempts) VALUES(?,?,?,'running',?,?,1) ON CONFLICT(project_id,day) DO UPDATE SET status='running',error=NULL,completed_at=NULL,next_attempt_at=NULL,mail_next_attempt_at=NULL,mail_error=NULL,mail_attempts=0,mail_status='pending',started_at=excluded.started_at,settings_json=excluded.settings_json,attempts=daily_briefings.attempts+1")
            .bind(&id).bind(project_id).bind(day).bind(Utc::now().to_rfc3339()).bind(settings.to_string()).execute(self.db.pool()).await?;
        let worker = self.clone();
        let run_id = id.clone();
        tokio::spawn(async move {
            let (cancel_tx, cancel_rx) = watch::channel(false);
            let generation = worker.generate(&run_id, &config, cancel_rx);
            tokio::pin!(generation);
            let error = tokio::select! {
                result = &mut generation => result.err().map(|error| {
                    let retry = retry_time(&error, attempt, Utc::now());
                    (format!("{error:#}"), !permanent_failure(&error), retry)
                }),
                _ = tokio::time::sleep(Duration::from_secs(config.timeout_minutes * 60)) => {
                    let _ = cancel_tx.send(true);
                    // Let the runtime interrupt and clean up the active turn before releasing it.
                    let _ = tokio::time::timeout(Duration::from_secs(30), &mut generation).await;
                    Some(("晨报生成超时；已请求停止本次执行".into(), true, Utc::now()+chrono::Duration::minutes(10 * (1i64 << attempt.saturating_sub(1).min(3)))))
                }
            };
            if let Some((error, retryable, retry_at)) = error {
                let retry = (retryable && attempt < 3).then(|| retry_at.to_rfc3339());
                let _ = sqlx::query("UPDATE daily_briefings SET status='failed',error=?,next_attempt_at=?,completed_at=?,mail_status=CASE WHEN mail_status IN ('sent','uncertain','sending') THEN mail_status ELSE 'pending' END WHERE id=? AND status='running'").bind(error).bind(retry).bind(Utc::now().to_rfc3339()).bind(&run_id).execute(worker.db.pool()).await;
                let _ = sqlx::query("UPDATE daily_briefings SET settings_json=json_set(settings_json,'$.search_diagnostics.failure_stage',json_extract(settings_json,'$.search_diagnostics.stage'),'$.search_diagnostics.stage','failed') WHERE id=?").bind(&run_id).execute(worker.db.pool()).await;
            }
        });
        Ok(id)
    }
    async fn diagnostic(&self, id: &str, key: &str, value: Value) -> Result<()> {
        sqlx::query(
            "UPDATE daily_briefings SET settings_json=json_set(settings_json,?,json(?)) WHERE id=?",
        )
        .bind(format!("$.search_diagnostics.{key}"))
        .bind(serde_json::to_string(&value)?)
        .bind(id)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
    async fn fetch(
        &self,
        id: &str,
        phase: &str,
        since: DateTime<Utc>,
        query: &str,
        remaining_pages: &mut usize,
    ) -> Result<(Vec<WorkMetadata>, usize)> {
        let client = research_http_client()?;
        let mut all = Vec::new();
        let mut audit = crate::briefing_retrieval::FetchAudit::new(query);
        self.diagnostic(id, phase, json!(&audit)).await?;
        for _ in 0..10 {
            let cache = crate::briefing_retrieval::cache_path(
                &self.workspace,
                id,
                query,
                audit.received,
                since,
            );
            let started = std::time::Instant::now();
            let page = if let Some(page) =
                crate::briefing_retrieval::cached_page(&cache, Utc::now()).await
            {
                audit.cached_pages += 1;
                audit.last_request_ms = 0;
                page
            } else {
                if *remaining_pages == 0 {
                    bail!("晨报共享检索预算已用完，未确认所有主题覆盖；不视作无新增");
                }
                *remaining_pages -= 1;
                let mut url = url::Url::parse("https://export.arxiv.org/api/query")?;
                url.query_pairs_mut().extend_pairs([
                    ("search_query", query.to_owned()),
                    ("sortBy", "lastUpdatedDate".into()),
                    ("sortOrder", "descending".into()),
                    ("start", audit.received.to_string()),
                    ("max_results", "200".into()),
                ]);
                let response = crate::arxiv_http::request(
                    &client,
                    url,
                    Duration::from_secs(crate::briefing_retrieval::REQUEST_TIMEOUT_SECONDS),
                )
                .await;
                audit.last_request_ms = started.elapsed().as_millis() as u64;
                let (body, diagnostic) = match response {
                    Ok(result) => result,
                    Err(error) => {
                        audit.error_kind =
                            Some(crate::briefing_retrieval::error_kind(&error).into());
                        audit.response = error
                            .downcast_ref::<crate::arxiv_http::ArxivError>()
                            .map(|error| error.0.clone());
                        if audit
                            .response
                            .as_ref()
                            .is_some_and(|response| response.cooldown_blocked)
                        {
                            *remaining_pages += 1;
                        }
                        self.diagnostic(id, phase, json!(&audit)).await?;
                        self.diagnostic(id, "last_response", json!(&audit.response))
                            .await?;
                        return Err(error);
                    }
                };
                crate::briefing_retrieval::CachedPage {
                    fetched_at: Utc::now(),
                    entries: parse_arxiv_search(&body)?,
                    total: crate::briefing_retrieval::total_results(&body)?,
                    diagnostic,
                }
            };
            audit.response = Some(page.diagnostic.clone());
            audit.last_fetched_at = Some(page.fetched_at);
            let accepted = audit.page(page.entries.clone(), page.total, since);
            self.diagnostic(id, phase, json!(&audit)).await?;
            all.extend(accepted?);
            // Retain verified pages across scheduled retries; malformed/failed
            // responses never enter this cache. No change to the seen cursor.
            if let Err(error) = atomic_write(&cache, &serde_json::to_vec(&page)?).await {
                tracing::warn!(%error,"could not cache validated briefing page");
            }
            if audit.complete {
                return Ok((all, audit.cached_pages));
            }
        }
        audit.limit_reached = true;
        self.diagnostic(id, phase, json!(&audit)).await?;
        bail!("检索达到 10 页 / 2000 篇上限，无法确认完整覆盖；未当作无新增，未推进检索位置")
    }
    async fn unseen(
        &self,
        project_id: &str,
        papers: Vec<WorkMetadata>,
    ) -> Result<Vec<WorkMetadata>> {
        let mut unseen = Vec::new();
        let mut included = std::collections::BTreeSet::new();
        for paper in papers {
            let updated = paper.metadata["updated"].as_str().unwrap_or_default();
            if !included.insert((paper.canonical_key.clone(), updated.to_owned())) {
                continue;
            }
            let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM briefing_seen WHERE project_id=? AND paper_id=? AND updated=?")
                .bind(project_id).bind(&paper.canonical_key).bind(updated).fetch_one(self.db.pool()).await?;
            if seen == 0 {
                unseen.push(paper);
            }
        }
        Ok(unseen)
    }
    async fn generate(
        &self,
        id: &str,
        config: &BriefingConfig,
        cancel: watch::Receiver<bool>,
    ) -> Result<()> {
        let day = self.get(id).await?.day;
        let previous: Option<String> = sqlx::query_scalar(
            "SELECT max(started_at) FROM daily_briefings WHERE project_id=? AND status IN ('completed','empty')",
        )
        .bind(&config.project_id)
        .fetch_one(self.db.pool())
        .await?;
        let since = crate::briefing_retrieval::since(previous.as_deref(), Utc::now());
        self.diagnostic(id, "since", json!(since.to_rfc3339()))
            .await?;
        self.diagnostic(id, "stage", json!("planning")).await?;
        let mut context =
            crate::briefing_project::context(&self.db, &self.workspace, &config.project_id).await?;
        let plan = crate::briefing_project::plan(&self.codex, &self.workspace, &context, &config.keywords, cancel.clone()).await.map_err(|error| {
            tracing::warn!(%error,"project briefing search planning failed; no automatic model retries");
            ModelRejected
        })?.normalized();
        let query = plan.query(&config.categories);
        let search_plan = json!({"terms":plan.terms,"topics":plan.topics,"dispositions":plan.dispositions,"rationale":plan.rationale,"query":query,"project_name":context["project"]["name"]});
        sqlx::query("UPDATE daily_briefings SET settings_json=json_set(settings_json,'$.search_plan',json(?)) WHERE id=?")
            .bind(serde_json::to_string(&search_plan)?)
            .bind(id)
            .execute(self.db.pool())
            .await?;
        self.diagnostic(id, "stage", json!("retrieving")).await?;
        let mut remaining_pages = 24usize;
        let mut papers = Vec::new();
        let mut primary_failed = false;
        let mut retrievals = Vec::new();
        for (index, topic) in plan.topics.iter().enumerate() {
            if index > 0 {
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
            self.diagnostic(id, "active_topic", json!(topic.label))
                .await?;
            let topic_query =
                crate::briefing_project::SearchPlan::query_terms(&config.categories, &topic.terms);
            let before = remaining_pages;
            let result = self
                .fetch(
                    id,
                    &format!("topic_{index}"),
                    since,
                    &topic_query,
                    &mut remaining_pages,
                )
                .await;
            match result {
                Ok((found, cached_pages)) => {
                    retrievals.push(json!({"id":topic.id,"label":topic.label,"status":"complete","within_window":found.len(),"pages":before-remaining_pages,"cached_pages":cached_pages}));
                    papers.extend(found);
                }
                Err(error) => {
                    retrievals.push(json!({"id":topic.id,"label":topic.label,"status":"failed","error_kind":crate::briefing_retrieval::error_kind(&error),"pages":before-remaining_pages}));
                    self.diagnostic(id, "topic_retrievals", json!(&retrievals))
                        .await?;
                    if !crate::briefing_retrieval::fallback_eligible(&error) {
                        return Err(error);
                    }
                    primary_failed = true;
                }
            }
            self.diagnostic(id, "topic_retrievals", json!(&retrievals))
                .await?;
        }
        let primary_count = papers.len();
        let mut unseen = self.unseen(&config.project_id, papers).await?;
        self.diagnostic(id, "primary_selection", json!({"retrieved":primary_count,"unseen":unseen.len(),"candidates":crate::briefing_editorial::select(&unseen, &plan, config.max_papers).len()})).await?;
        let mut reviewed_empty = false;
        // A shared category review is bounded and runs at most once, even when
        // several themes have no unseen matches or suffered transient failure.
        if primary_failed
            || plan.topics.iter().any(|topic| {
                crate::briefing_editorial::candidates(&unseen, &topic.terms, 1).is_empty()
            })
        {
            self.diagnostic(
                id,
                "stage",
                json!(if primary_failed {
                    "fallback_retrieval"
                } else {
                    "empty_review"
                }),
            )
            .await?;
            tokio::time::sleep(Duration::from_secs(3)).await;
            let fallback_query = config
                .categories
                .iter()
                .map(|category| format!("cat:{category}"))
                .collect::<Vec<_>>()
                .join(" OR ");
            let (fallback, _) = self
                .fetch(id, "fallback", since, &fallback_query, &mut remaining_pages)
                .await?;
            let fallback_count = fallback.len();
            unseen.extend(fallback);
            unseen = self.unseen(&config.project_id, unseen).await?;
            self.diagnostic(id, "fallback_selection", json!({"retrieved":fallback_count,"unseen":unseen.len(),"candidates":crate::briefing_editorial::select(&unseen, &plan, config.max_papers).len()})).await?;
            reviewed_empty = true;
        }
        let candidates = crate::briefing_editorial::select(&unseen, &plan, config.max_papers);
        let coverage = crate::briefing_editorial::coverage(&unseen, &candidates, &plan);
        self.diagnostic(id, "coverage", coverage.clone()).await?;
        self.diagnostic(id, "dispositions", json!(plan.dispositions))
            .await?;
        self.diagnostic(id, "request_pages_used", json!(24 - remaining_pages))
            .await?;
        context["briefing_coverage"] = coverage;
        context["briefing_search_plan"] = json!(plan);
        context["briefing_editorial_skill"] =
            json!(crate::briefing_skill::instructions(&self.workspace, false).await?);
        self.diagnostic(id, "empty_reviewed", json!(reviewed_empty))
            .await?;
        self.diagnostic(id, "selected_candidates", json!(candidates.len()))
            .await?;
        self.diagnostic(
            id,
            "stage",
            json!(if candidates.is_empty() {
                "verified_empty"
            } else {
                "collecting_evidence"
            }),
        )
        .await?;
        let mut sources = Vec::new();
        for (index, paper) in candidates.iter().enumerate() {
            let mut entry = json!({"paper":paper,"evidence":"abstract","fulltext":null});
            entry["research_topics"] = json!(plan.topics.iter().filter(|topic| crate::briefing_editorial::topic_score(paper, &topic.terms)>0).map(|topic| json!({"id":topic.id,"label":topic.label,"project_ids":topic.project_ids})).collect::<Vec<_>>());
            if index < config.fulltext_papers {
                let work = self.research.store().upsert_work((*paper).clone()).await?;
                if let Ok(Ok(inspected)) = tokio::time::timeout(
                    Duration::from_secs(80),
                    self.research.inspect(&work.id, true),
                )
                .await
                {
                    entry["evidence"] = json!(inspected.evidence_level);
                    let (excerpt, truncated) =
                        crate::briefing_editorial::evidence_excerpt(&inspected.text);
                    entry["fulltext"] = json!(excerpt);
                    entry["truncated"] = json!(truncated);
                    entry["excerpt_strategy"] = json!(if truncated {
                        "intro_method_results_conclusion_windows_with_explicit_gaps"
                    } else {
                        "complete_available_text"
                    });
                    entry["evidence_url"] = json!(inspected.source_url);
                }
            }
            sources.push(entry);
        }
        // Optional presentation evidence must not prevent a text briefing.
        let media_cache = self.workspace.state_dir().join("briefing-assets");
        let _ = tokio::time::timeout(Duration::from_secs(120), async {
            for source in &mut sources {
                if let Ok(metadata) =
                    crate::briefing_media::metadata(&source["paper"], &media_cache).await
                {
                    source["presentation"] = metadata;
                }
            }
        })
        .await;
        let mut markdown = if sources.is_empty() {
            format!("## 今日晨报：暂无新增\n\n已完成项目主题检索及分类范围复查，本次未选出符合项目方向、且尚未介绍的论文。**这不表示今天没有论文发表。**\n\n检索窗口起点：{}（UTC）。已核对窗口、去重和主题筛选；具体检索数量与条件可在网页「检索记录」查看。\n\narXiv 索引可能延迟；后续晨报保留 7 天重叠窗口复查。今天仍发送本状态简报，不静默跳过。", since.format("%Y-%m-%d %H:%M"))
        } else {
            self.diagnostic(id, "stage", json!("writing")).await?;
            let global = self
                .db
                .list_memory_items("global", None, &["preference"])
                .await?;
            let prompt = crate::briefing_editorial::prompt(
                &day,
                &since.to_rfc3339(),
                &context,
                &json!(global.into_iter().take(12).collect::<Vec<_>>()),
                &sources,
            );
            let cwd = self.workspace.state_dir().join("briefing-work");
            tokio::fs::create_dir_all(&cwd).await?;
            let outcome = self
                .codex
                .run_turn(
                    CodexTurn {
                        thread_id: None,
                        cwd,
                        prompt,
                        skill: Some(crate::briefing_skill::selection(&self.workspace)),
                        tool_preferences: vec![],
                        output_schema: None,
                        settings: self.codex.research_conversation_settings(),
                    },
                    cancel.clone(),
                )
                .await?;
            if outcome.status != "completed" || outcome.final_text.trim().is_empty() {
                if outcome.is_retryable_failure() {
                    bail!("晨报模型上游暂时不可用");
                }
                return Err(ModelRejected.into());
            }
            outcome.final_text
        };
        // Keep omissions observable without throwing away a useful answer or
        // initiating extra model retries merely to satisfy a layout template.
        let published = selected_works(&markdown, &sources);
        let publication: Vec<_> = plan.topics.iter().map(|topic| {
            let count = published.iter().filter(|paper| crate::briefing_editorial::topic_score(paper, &topic.terms)>0).count();
            let available = unseen.iter().filter(|paper| crate::briefing_editorial::topic_score(paper, &topic.terms)>0).count();
            if available > 0 && count == 0 {
                markdown.push_str(&format!("\n\n覆盖提醒：{} 检索到 {} 篇未介绍的相关候选，本期正文未展开；不能据此视为该方向没有新增。", topic.label, available));
            }
            json!({"id":topic.id,"label":topic.label,"published":count})
        }).collect();
        for disposition in plan
            .dispositions
            .iter()
            .filter(|item| item.kind != "organizational")
        {
            let label = context["structure"]
                .as_array()
                .and_then(|nodes| {
                    nodes
                        .iter()
                        .find(|node| node["id"] == disposition.project_id)
                })
                .and_then(|node| node["name"].as_str())
                .unwrap_or(&disposition.project_id);
            markdown.push_str(&format!(
                "\n\n范围说明：{label} — {}。本次未据此宣称完成该方向检索。",
                disposition.reason
            ));
        }
        self.diagnostic(id, "publication_coverage", json!(publication))
            .await?;
        if *cancel.borrow() {
            bail!("晨报生成已取消");
        }
        persist_briefing(&self.db, id, &markdown, &sources, config.email_enabled).await?;
        let _ = self
            .diagnostic(
                id,
                "stage",
                json!(if sources.is_empty() {
                    "verified_empty"
                } else {
                    "completed"
                }),
            )
            .await;
        // Chat archival is optional and must never discard a completed digest.
        if !sources.is_empty() {
            if let Err(error) = self.archive_briefing(id, &day, config, &markdown).await {
                tracing::warn!(%error, briefing_id=id, "briefing saved but project chat archival failed");
            }
        }
        Ok(())
    }
    async fn archive_briefing(
        &self,
        id: &str,
        day: &str,
        config: &BriefingConfig,
        markdown: &str,
    ) -> Result<()> {
        let Some(scope) = briefing_chat_scope(config) else {
            return Ok(());
        };
        let conversation = self
            .conversations
            .create_conversation(&format!("{day} 论文晨报"), vec![scope])
            .await?;
        self.db
            .append_chat_message(
                &conversation.id,
                "user",
                "请根据本次提供的论文证据生成晨报",
                "completed",
            )
            .await?;
        self.db
            .append_chat_message(&conversation.id, "assistant", markdown, "completed")
            .await?;
        sqlx::query("UPDATE daily_briefings SET conversation_id=? WHERE id=?")
            .bind(conversation.id)
            .bind(id)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }
    pub async fn deliver(
        &self,
        id: &str,
        manual: bool,
        expected_attempt: Option<i64>,
    ) -> Result<()> {
        let _guard = self.gate.lock().await;
        let item = self.get(id).await?;
        // Two requests based on the same displayed state must not turn a single
        // click/retry into a second send after the first attempt returns uncertain.
        if expected_attempt.is_some_and(|attempt| attempt != item.mail_attempts) {
            return Ok(());
        }
        let config = self.config(&item.project_id).await?;
        let project = self
            .db
            .get_project(&item.project_id)
            .await?
            .context("晨报所属项目已删除")?;
        if !can_deliver(&item) || !config.email_enabled {
            bail!("仅发送已完成晨报、无新增状态或已结束重试的失败通知，请先启用邮件");
        }
        if item.mail_status == "sent"
            || item.mail_attempts >= 3
            || (!manual && matches!(item.mail_status.as_str(), "uncertain" | "blocked"))
        {
            return Ok(());
        }
        let env_path = self.mail_env_path.as_ref().context("未配置邮件凭据文件")?;
        sqlx::query("UPDATE daily_briefings SET mail_status='sending',mail_attempts=mail_attempts+1 WHERE id=?").bind(id).execute(self.db.pool()).await?;
        let sources: Vec<Value> = serde_json::from_str(&item.sources_json).unwrap_or_default();
        let body = delivery_body(&item);
        let label = match item.status.as_str() {
            "empty" => "论文晨报 · 暂无新增",
            "failed" => "论文晨报 · 生成失败通知",
            _ => "论文晨报",
        };
        let payload = json!({"recipient":config.recipient,"subject":format!("{} · {} · {}",item.day,project.name,label),"body":body,"html_body":crate::briefing_email::render_with_sources(&format!("{} · {}",item.day,project.name), &body, &sources),"message_id":format!("<briefing-{}-{}@paper-codex.local>",item.id,item.attempts)});
        let result = self.send_mail(env_path, payload).await;
        let (status, error) = match result {
            Ok(0) => ("sent", None),
            Ok(1) => (
                "failed",
                Some("SMTP 临时失败，10 分钟后重试；最多 3 次".to_string()),
            ),
            Ok(3) => (
                "blocked",
                Some("发信配置或授权被拒绝，请检查本地凭据后手动重发".to_string()),
            ),
            Ok(4) => (
                "blocked",
                Some(
                    "SMTP 服务明确拒收邮件正文（5xx），未投递；请检查发信规则后手动重发"
                        .to_string(),
                ),
            ),
            _ => (
                "uncertain",
                Some("邮件发送结果不确定，请检查邮箱后决定是否重发".to_string()),
            ),
        };
        sqlx::query(
            "UPDATE daily_briefings SET mail_status=?,mail_error=?,mail_next_attempt_at=? WHERE id=?",
        )
        .bind(status)
        .bind(error)
        .bind((Utc::now() + chrono::Duration::minutes(10)).to_rfc3339())
        .bind(id)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
    async fn send_mail(&self, env_path: &std::path::Path, payload: Value) -> Result<i32> {
        let mut command = tokio::process::Command::new("python3");
        command
            .arg("-c")
            .arg(include_str!("../scripts/send-briefing-mail.py"))
            .arg(env_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().context("无法启动邮件发送器")?;
        let mut stdin = child.stdin.take().context("邮件输入不可用")?;
        stdin.write_all(&serde_json::to_vec(&payload)?).await?;
        drop(stdin);
        let status = tokio::time::timeout(Duration::from_secs(90), child.wait())
            .await
            .context("邮件发送超时，请检查邮箱后重试")??;
        Ok(status.code().unwrap_or(2))
    }
}

fn retry_allowed(item: &Briefing, manual: bool, now: &str) -> bool {
    let recheck_empty = manual && item.status == "empty" && item.mail_attempts == 0;
    (item.status == "failed" || recheck_empty)
        && (manual
            || (item.attempts < 3
                && item
                    .next_attempt_at
                    .as_deref()
                    .is_some_and(|date| date <= now)))
}

fn attempt_settings(config: &BriefingConfig, previous: Option<&Briefing>) -> Result<Value> {
    let mut settings = serde_json::to_value(config)?;
    if let Some(previous) = previous {
        let old = serde_json::from_str::<Value>(&previous.settings_json).unwrap_or_default();
        let mut history = old["attempt_history"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        history.push(json!({
            "attempt": previous.attempts, "status": previous.status,
            "started_at": previous.started_at, "completed_at": previous.completed_at,
            "error": previous.error, "search_diagnostics": old["search_diagnostics"],
            "mail_status": previous.mail_status, "mail_attempts": previous.mail_attempts,
        }));
        settings["attempt_history"] = json!(history);
    }
    Ok(settings)
}

pub(crate) fn can_deliver(item: &Briefing) -> bool {
    matches!(item.status.as_str(), "completed" | "empty")
        || (item.status == "failed" && (item.attempts >= 3 || item.next_attempt_at.is_none()))
}

pub(crate) fn delivery_body(item: &Briefing) -> String {
    if item.status == "failed" {
        // Provider errors can include sensitive request details. Keep the email
        // notification factual and direct the owner to the authenticated UI.
        let settings = serde_json::from_str::<Value>(&item.settings_json).unwrap_or_default();
        let phase = match settings["search_diagnostics"]["failure_stage"].as_str() {
            Some("planning") => "检索规划",
            Some("retrieving") => "项目检索",
            Some("empty_review") => "空结果复查",
            Some("fallback_retrieval") => "分类降级检索",
            Some("collecting_evidence") => "证据收集",
            Some("writing") => "晨报撰写",
            _ => "检索或生成",
        };
        format!("## 今日晨报未能完成\n\n出错阶段：{phase}。已结束自动重试或遇到不可自动重试的错误（已尝试 {} 次）。\n\n**这不是“没有新增论文”。** 本邮件仅通知任务异常，没有生成正常论文晨报。\n\n请打开 Paper Codex 的「论文晨报」栏目查看失败原因和检索记录，再决定是否手动重试。", item.attempts)
    } else if item.status == "empty"
        && serde_json::from_str::<Value>(&item.settings_json).unwrap_or_default()
            ["search_diagnostics"]["empty_reviewed"]
            != json!(true)
    {
        "## 今日晨报状态：空结果尚未核验\n\n旧版任务留下了空结果，但没有保存完整的检索和复查记录。**不能据此认定没有新增论文。**\n\n请在网页「论文晨报」查看记录，尚未投递时可重新检索。此邮件是状态提醒，不是正常论文晨报。".into()
    } else {
        item.markdown.clone()
    }
}

fn briefing_chat_scope(config: &BriefingConfig) -> Option<ConversationScopeInput> {
    if config.project_id.is_empty() {
        return None;
    }
    Some(ConversationScopeInput {
        scope_type: "project".into(),
        scope_id: Some(config.project_id.clone()),
    })
}

async fn persist_briefing(
    db: &Database,
    id: &str,
    markdown: &str,
    sources: &[Value],
    email_enabled: bool,
) -> Result<()> {
    let mut transaction = db.pool().begin_with("BEGIN IMMEDIATE").await?;
    let project_id: String =
        sqlx::query_scalar("SELECT project_id FROM daily_briefings WHERE id=?")
            .bind(id)
            .fetch_one(&mut *transaction)
            .await?;
    for paper in selected_works(markdown, sources) {
        sqlx::query(
            "INSERT OR IGNORE INTO briefing_seen(project_id,paper_id,updated) VALUES(?,?,?)",
        )
        .bind(&project_id)
        .bind(&paper.canonical_key)
        .bind(paper.metadata["updated"].as_str().unwrap_or_default())
        .execute(&mut *transaction)
        .await?;
    }
    sqlx::query("UPDATE daily_briefings SET status=?,markdown=?,sources_json=?,error=NULL,completed_at=?,next_attempt_at=NULL,mail_next_attempt_at=NULL,mail_status=? WHERE id=?")
        .bind(if sources.is_empty() {"empty"} else {"completed"}).bind(markdown).bind(serde_json::to_string(sources)?).bind(Utc::now().to_rfc3339())
        .bind(if !email_enabled {"skipped"} else {"pending"}).bind(id).execute(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_scheduler_never_shortens_server_cooldown_and_keeps_permanent_errors_terminal() {
        let now = Utc::now();
        let limited: anyhow::Error = crate::arxiv_http::ArxivError(crate::arxiv_http::Diagnostic {
            http_status: Some(429),
            error_kind: Some("rate_limited".into()),
            retry_at: Some(now + chrono::Duration::hours(8)),
            ..Default::default()
        })
        .into();
        assert!(!permanent_failure(&limited));
        assert_eq!(
            retry_time(&limited, 1, now),
            now + chrono::Duration::hours(8)
        );
        let transient = anyhow::anyhow!("timeout");
        assert_eq!(
            retry_time(&transient, 1, now),
            now + chrono::Duration::minutes(10)
        );
        assert_eq!(
            retry_time(&transient, 2, now),
            now + chrono::Duration::minutes(20)
        );
        let forbidden: anyhow::Error =
            crate::arxiv_http::ArxivError(crate::arxiv_http::Diagnostic {
                http_status: Some(403),
                error_kind: Some("http_rejected".into()),
                ..Default::default()
            })
            .into();
        assert!(permanent_failure(&forbidden));
    }

    #[test]
    fn importable_papers_are_only_linked_works_in_the_actual_briefing() {
        let works =
            parse_arxiv_search(include_str!("../fixtures/research/arxiv-search.xml")).unwrap();
        let sources = vec![
            json!({"paper":works[0]}),
            json!({"paper":works[1]}),
            json!({"paper":works[0]}),
            json!({"paper":null}),
        ];
        let markdown = format!(
            "[论文]({})\n[外部链接](https://example.test/other)",
            works[0].source_url
        );
        let selected = selected_works(&markdown, &sources);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].canonical_key, works[0].canonical_key);
        assert!(selected_works("没有引用候选论文", &sources).is_empty());
        let legacy = format!("- **链接**：{}\n", works[0].source_url);
        assert_eq!(selected_works(&legacy, &sources).len(), 1);
        assert_eq!(
            selected_works(&format!("原文：{}。", works[0].source_url), &sources).len(),
            1
        );
        assert!(selected_works(&format!("{}999", works[0].source_url), &sources).is_empty());
        assert!(
            selected_works(&format!("```text\n{}\n```", works[0].source_url), &sources).is_empty()
        );
    }

    #[test]
    fn briefing_chat_scope_is_always_the_owned_project() {
        let mut config = BriefingConfig::default();
        assert!(briefing_chat_scope(&config).is_none());
        config.project_id = "project-a".into();
        let scope = briefing_chat_scope(&config).unwrap();
        assert_eq!(scope.scope_type, "project");
        assert_eq!(scope.scope_id.as_deref(), Some("project-a"));
    }

    #[tokio::test]
    async fn completed_body_and_delivery_are_persisted_before_optional_archival() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        BriefingService::recover_states(&db).await.unwrap();
        sqlx::query("INSERT INTO daily_briefings(id,project_id,day,status,started_at,settings_json) VALUES('digest','project-a','2026-09-27','running','2026-09-27','{}')").execute(db.pool()).await.unwrap();
        let works =
            parse_arxiv_search(include_str!("../fixtures/research/arxiv-search.xml")).unwrap();
        let markdown = format!("# 已生成的正文\n\n[论文]({})", works[0].source_url);
        persist_briefing(&db, "digest", &markdown, &[json!({"paper":works[0]})], true)
            .await
            .unwrap();
        // This is also the state retained if optional archival later fails.
        let item: Briefing = sqlx::query_as("SELECT * FROM daily_briefings WHERE id='digest'")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(item.status, "completed");
        assert_eq!(item.markdown, markdown);
        assert_eq!(item.project_id, "project-a");
        assert_eq!(item.mail_status, "pending");
        assert!(item.conversation_id.is_none());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM briefing_seen")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn empty_days_notify_and_failure_mail_cannot_restart_generation() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        BriefingService::recover_states(&db).await.unwrap();
        sqlx::query("INSERT INTO daily_briefings(id,project_id,day,status,started_at,settings_json,attempts) VALUES('empty','ei','2026-09-28','running','2026-09-28','{}',1)").execute(db.pool()).await.unwrap();
        persist_briefing(&db, "empty", "暂无新增", &[], true)
            .await
            .unwrap();
        let mut item: Briefing = sqlx::query_as("SELECT * FROM daily_briefings WHERE id='empty'")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(item.status, "empty");
        assert_eq!(item.mail_status, "pending");
        assert!(can_deliver(&item));
        assert!(delivery_body(&item).contains("尚未核验"));
        item.settings_json = json!({"search_diagnostics":{"empty_reviewed":true}}).to_string();
        assert_eq!(delivery_body(&item), "暂无新增");
        item.status = "failed".into();
        item.next_attempt_at = Some("2099-01-01".into());
        assert!(!can_deliver(&item));
        item.attempts = 3;
        assert!(can_deliver(&item));
        assert!(!retry_allowed(&item, false, "2100-01-01"));
        assert!(retry_allowed(&item, true, "2100-01-01"));
        item.mail_status = "sent".into();
        item.mail_attempts = 1;
        let settings = attempt_settings(&BriefingConfig::default(), Some(&item)).unwrap();
        assert_eq!(settings["attempt_history"][0]["attempt"], 3);
        assert_eq!(settings["attempt_history"][0]["mail_status"], "sent");
        assert_eq!(settings["attempt_history"][0]["mail_attempts"], 1);
        item.settings_json = settings.to_string();
        item.attempts = 4;
        assert!(!retry_allowed(&item, false, "2100-01-01"));
        let settings = attempt_settings(&BriefingConfig::default(), Some(&item)).unwrap();
        assert_eq!(settings["attempt_history"].as_array().unwrap().len(), 2);
        item.status = "completed".into();
        assert!(!retry_allowed(&item, true, "2100-01-01"));
        item.status = "failed".into();
        item.error = Some("sensitive-provider-detail".into());
        assert!(!delivery_body(&item).contains("sensitive-provider-detail"));
        item.attempts = 1;
        item.next_attempt_at = None;
        assert!(can_deliver(&item));
        sqlx::query("UPDATE daily_briefings SET status='failed',next_attempt_at=NULL,mail_next_attempt_at='2099-01-01',mail_status='failed' WHERE id='empty'").execute(db.pool()).await.unwrap();
        let after_mail_retry: Briefing =
            sqlx::query_as("SELECT * FROM daily_briefings WHERE id='empty'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert!(after_mail_retry.next_attempt_at.is_none());
        assert!(after_mail_retry.mail_next_attempt_at.is_some());
        assert!(can_deliver(&after_mail_retry));
    }
}

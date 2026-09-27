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
 id TEXT PRIMARY KEY, day TEXT NOT NULL UNIQUE, status TEXT NOT NULL,
 markdown TEXT NOT NULL DEFAULT '', error TEXT, conversation_id TEXT,
 mail_status TEXT NOT NULL DEFAULT 'pending', mail_attempts INTEGER NOT NULL DEFAULT 0,
 attempts INTEGER NOT NULL DEFAULT 0, started_at TEXT NOT NULL, completed_at TEXT,
 sources_json TEXT NOT NULL DEFAULT '[]', settings_json TEXT NOT NULL,
 next_attempt_at TEXT, mail_error TEXT
);
CREATE TABLE IF NOT EXISTS briefing_seen (paper_id TEXT NOT NULL, updated TEXT NOT NULL,
 PRIMARY KEY(paper_id, updated));
"#;

#[derive(Debug, thiserror::Error)]
#[error("模型拒绝或无法完成晨报，请检查模型提供商/凭据后手动重试；未进行自动重试")]
struct ModelRejected;

fn permanent_failure(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ModelRejected>().is_some()
        || error
            .downcast_ref::<reqwest::Error>()
            .and_then(|e| e.status())
            .is_some_and(|status| status.is_client_error() && status.as_u16() != 429)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BriefingConfig {
    pub enabled: bool,
    pub time: String,
    pub timezone: String,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    pub project_ids: Vec<String>,
    pub max_papers: usize,
    pub fulltext_papers: usize,
    pub timeout_minutes: u64,
    pub email_enabled: bool,
    pub recipient: String,
}
impl Default for BriefingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            time: "09:30".into(),
            timezone: "Asia/Shanghai".into(),
            categories: vec!["cs.RO".into(), "cs.CV".into(), "cs.AI".into()],
            keywords: [
                "vision-language-action",
                "world model",
                "robot",
                "embodied",
                "action token",
                "long-horizon",
                "manipulation",
                "humanoid",
            ]
            .map(String::from)
            .to_vec(),
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
    pub fn validate(&self) -> Result<()> {
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
        if self.keywords.is_empty()
            || self.keywords.len() > 40
            || self
                .keywords
                .iter()
                .any(|v| v.trim().is_empty() || v.len() > 100)
            || self.project_ids.len() > 10
        {
            bail!("关注词须为 1–40 个，项目最多 10 个");
        }
        if self.email_enabled && !valid_mailbox(&self.recipient) {
            bail!("请填写有效收件邮箱");
        }
        Ok(())
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
        sqlx::raw_sql(SCHEMA).execute(db.pool()).await?;
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
                serde_yaml::to_string(&BriefingConfig::default())?.as_bytes(),
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
    pub async fn config(&self) -> Result<BriefingConfig> {
        let bytes = tokio::fs::read(&self.config_path)
            .await
            .context("读取晨报配置失败")?;
        let config: BriefingConfig =
            serde_yaml::from_slice(&bytes).context("晨报 YAML 配置无效")?;
        config.validate()?;
        Ok(config)
    }
    pub async fn save_config(&self, config: BriefingConfig) -> Result<()> {
        config.validate()?;
        for id in &config.project_ids {
            if self.db.get_project(id).await?.is_none() {
                bail!("晨报项目不存在");
            }
        }
        if config.email_enabled && !self.mail_configured() {
            bail!("尚未配置本地发信凭据文件");
        }
        let _guard = self.config_gate.lock().await;
        atomic_write(
            &self.config_path,
            serde_yaml::to_string(&config)?.as_bytes(),
        )
        .await
    }
    pub async fn list(&self) -> Result<Vec<Briefing>> {
        // Poll only small status records. Fetch the chosen day's body separately.
        Ok(sqlx::query_as("SELECT id,day,status,'' AS markdown,error,conversation_id,mail_status,mail_attempts,attempts,started_at,completed_at,'[]' AS sources_json,'{}' AS settings_json,next_attempt_at,mail_error FROM daily_briefings ORDER BY day DESC LIMIT 60").fetch_all(self.db.pool()).await?)
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
        let config = self.config().await?;
        if due(&config, Utc::now()) {
            self.launch(false).await?;
        }
        if config.email_enabled {
            let items: Vec<String> = sqlx::query_scalar("SELECT id FROM daily_briefings WHERE status='completed' AND mail_status IN ('pending','failed') AND mail_attempts<3 AND (next_attempt_at IS NULL OR next_attempt_at<=?) ORDER BY day LIMIT 1")
                .bind(Utc::now().to_rfc3339()).fetch_all(self.db.pool()).await?;
            for id in items {
                self.deliver(&id, false, None).await?;
            }
        }
        Ok(())
    }
    pub async fn launch(self: &Arc<Self>, manual: bool) -> Result<String> {
        let config = self.config().await?;
        let _guard = self.gate.lock().await;
        let day = local_day(Utc::now());
        let old: Option<Briefing> = sqlx::query_as("SELECT * FROM daily_briefings WHERE day=?")
            .bind(&day)
            .fetch_optional(self.db.pool())
            .await?;
        if let Some(ref item) = old {
            if item.status != "failed"
                || item.attempts >= 3
                || (!manual
                    && item
                        .next_attempt_at
                        .as_ref()
                        .is_some_and(|v| v > &Utc::now().to_rfc3339()))
            {
                return Ok(item.id.clone());
            }
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
        let id = old
            .map(|v| v.id)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        sqlx::query("INSERT INTO daily_briefings(id,day,status,started_at,settings_json,attempts) VALUES(?,?,'running',?,?,1) ON CONFLICT(day) DO UPDATE SET status='running',error=NULL,started_at=excluded.started_at,settings_json=excluded.settings_json,attempts=daily_briefings.attempts+1")
            .bind(&id).bind(day).bind(Utc::now().to_rfc3339()).bind(serde_json::to_string(&config)?).execute(self.db.pool()).await?;
        let worker = self.clone();
        let run_id = id.clone();
        tokio::spawn(async move {
            let (cancel_tx, cancel_rx) = watch::channel(false);
            let generation = worker.generate(&run_id, &config, cancel_rx);
            tokio::pin!(generation);
            let error = tokio::select! {
                result = &mut generation => result.err().map(|error| (format!("{error:#}"), !permanent_failure(&error))),
                _ = tokio::time::sleep(Duration::from_secs(config.timeout_minutes * 60)) => {
                    let _ = cancel_tx.send(true);
                    // Let the runtime interrupt and clean up the active turn before releasing it.
                    let _ = tokio::time::timeout(Duration::from_secs(30), &mut generation).await;
                    Some(("晨报生成超时；已请求停止本次执行".into(), true))
                }
            };
            if let Some((error, retryable)) = error {
                let retry =
                    retryable.then(|| (Utc::now() + chrono::Duration::minutes(10)).to_rfc3339());
                let _ = sqlx::query("UPDATE daily_briefings SET status='failed',error=?,next_attempt_at=? WHERE id=? AND status='running'").bind(error).bind(retry).bind(&run_id).execute(worker.db.pool()).await;
            }
        });
        Ok(id)
    }
    async fn fetch(
        &self,
        since: DateTime<Utc>,
        config: &BriefingConfig,
    ) -> Result<Vec<WorkMetadata>> {
        let client = research_http_client()?;
        let query = config
            .categories
            .iter()
            .map(|v| format!("cat:{v}"))
            .collect::<Vec<_>>()
            .join(" OR ");
        let mut all = Vec::new();
        for page in 0..10 {
            if page > 0 {
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
            let response = client
                .get("https://export.arxiv.org/api/query")
                .query(&[
                    ("search_query", query.clone()),
                    ("sortBy", "lastUpdatedDate".into()),
                    ("sortOrder", "descending".into()),
                    ("start", (page * 200).to_string()),
                    ("max_results", "200".into()),
                ])
                .send()
                .await?
                .error_for_status()?;
            let entries = parse_arxiv_search(&response.text().await?)?;
            let end = entries.len() < 200;
            let mut reached = false;
            for item in entries {
                let updated = item.metadata["updated"]
                    .as_str()
                    .context("arXiv 返回缺少更新时间")?;
                let date = DateTime::parse_from_rfc3339(updated)?;
                if date < since {
                    reached = true;
                    continue;
                }
                all.push(item);
            }
            if end || reached {
                return Ok(all);
            }
        }
        bail!("近期论文超过单次 2000 篇抓取上限；请缩小分类范围，未推进抓取位置")
    }
    async fn generate(
        &self,
        id: &str,
        config: &BriefingConfig,
        cancel: watch::Receiver<bool>,
    ) -> Result<()> {
        let day = self.get(id).await?.day;
        let previous: Option<String> = sqlx::query_scalar(
            "SELECT max(started_at) FROM daily_briefings WHERE status IN ('completed','empty')",
        )
        .fetch_one(self.db.pool())
        .await?;
        let since = previous
            .and_then(|v| DateTime::parse_from_rfc3339(&v).ok())
            .map(|v| v.with_timezone(&Utc) - chrono::Duration::days(2))
            .unwrap_or_else(|| Utc::now() - chrono::Duration::days(4));
        let papers = self.fetch(since, config).await?;
        let mut unseen = Vec::new();
        for paper in papers {
            let updated = paper.metadata["updated"].as_str().unwrap_or_default();
            let seen: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM briefing_seen WHERE paper_id=? AND updated=?",
            )
            .bind(&paper.canonical_key)
            .bind(updated)
            .fetch_one(self.db.pool())
            .await?;
            if seen == 0 {
                unseen.push(paper);
            }
        }
        let candidates =
            crate::briefing_editorial::candidates(&unseen, &config.keywords, config.max_papers);
        let mut sources = Vec::new();
        let mut context = Vec::new();
        for project_id in &config.project_ids {
            let project = self
                .db
                .get_project(project_id)
                .await?
                .context("所选项目已被删除")?;
            let memories = self
                .db
                .list_memory_items("project", Some(project_id), &[])
                .await?;
            let mut library = Vec::new();
            for id in self
                .db
                .project_paper_ids(project_id)
                .await?
                .into_iter()
                .take(50)
            {
                if let Some(paper) = self.db.get_paper(&id).await? {
                    library.push(json!({"id":paper.id,"title":paper.title}));
                }
            }
            context.push(json!({"project":project,"memories":memories.into_iter().take(12).collect::<Vec<_>>(),"existing_papers":library}));
        }
        for (index, paper) in candidates.iter().enumerate() {
            let mut entry = json!({"paper":paper,"evidence":"abstract","fulltext":null});
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
        let markdown = if sources.is_empty() {
            "本次没有符合关注方向的新增论文。已检查近期更新并排除重复论文。".into()
        } else {
            let global = self
                .db
                .list_memory_items("global", None, &["interest", "goal"])
                .await?;
            let prompt = crate::briefing_editorial::prompt(
                &day,
                &since.to_rfc3339(),
                &json!(context),
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
                        skill: None,
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
        if *cancel.borrow() {
            bail!("晨报生成已取消");
        }
        persist_briefing(
            &self.db,
            id,
            &markdown,
            &sources,
            &unseen,
            config.email_enabled,
        )
        .await?;
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
        let config = self.config().await?;
        if item.status != "completed" || !config.email_enabled {
            bail!("仅发送已完成的晨报，请先启用邮件");
        }
        if item.mail_status == "sent"
            || item.mail_attempts >= 3
            || (!manual && matches!(item.mail_status.as_str(), "uncertain" | "blocked"))
        {
            return Ok(());
        }
        let env_path = self.mail_env_path.as_ref().context("未配置邮件凭据文件")?;
        sqlx::query("UPDATE daily_briefings SET mail_status='sending',mail_attempts=mail_attempts+1 WHERE id=?").bind(id).execute(self.db.pool()).await?;
        let payload = json!({"recipient":config.recipient,"subject":format!("{} · 具身论文晨报",item.day),"body":item.markdown,"html_body":crate::briefing_email::render(&item.day, &item.markdown),"message_id":format!("<briefing-{}@paper-codex.local>",item.id)});
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
            "UPDATE daily_briefings SET mail_status=?,mail_error=?,next_attempt_at=? WHERE id=?",
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

fn briefing_chat_scope(config: &BriefingConfig) -> Option<ConversationScopeInput> {
    if config.project_ids.len() != 1 {
        return None;
    }
    Some(ConversationScopeInput {
        scope_type: "project".into(),
        scope_id: Some(config.project_ids[0].clone()),
    })
}

async fn persist_briefing(
    db: &Database,
    id: &str,
    markdown: &str,
    sources: &[Value],
    unseen: &[WorkMetadata],
    email_enabled: bool,
) -> Result<()> {
    let mut transaction = db.pool().begin_with("BEGIN IMMEDIATE").await?;
    for paper in unseen {
        sqlx::query("INSERT OR IGNORE INTO briefing_seen(paper_id,updated) VALUES(?,?)")
            .bind(&paper.canonical_key)
            .bind(paper.metadata["updated"].as_str().unwrap_or_default())
            .execute(&mut *transaction)
            .await?;
    }
    sqlx::query("UPDATE daily_briefings SET status=?,markdown=?,sources_json=?,error=NULL,completed_at=?,next_attempt_at=NULL,mail_status=? WHERE id=?")
        .bind(if sources.is_empty() {"empty"} else {"completed"}).bind(markdown).bind(serde_json::to_string(sources)?).bind(Utc::now().to_rfc3339())
        .bind(if sources.is_empty() || !email_enabled {"skipped"} else {"pending"}).bind(id).execute(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_and_cross_project_briefings_do_not_invent_a_global_chat_scope() {
        let mut config = BriefingConfig::default();
        assert!(briefing_chat_scope(&config).is_none());
        config.project_ids = vec!["project-a".into()];
        let scope = briefing_chat_scope(&config).unwrap();
        assert_eq!(scope.scope_type, "project");
        assert_eq!(scope.scope_id.as_deref(), Some("project-a"));
        config.project_ids.push("project-b".into());
        assert!(briefing_chat_scope(&config).is_none());
    }

    #[tokio::test]
    async fn completed_body_and_delivery_are_persisted_without_a_chat_project() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        BriefingService::recover_states(&db).await.unwrap();
        sqlx::query("INSERT INTO daily_briefings(id,day,status,started_at,settings_json) VALUES('digest','2026-09-27','running','2026-09-27','{}')").execute(db.pool()).await.unwrap();
        let works =
            parse_arxiv_search(include_str!("../fixtures/research/arxiv-search.xml")).unwrap();
        persist_briefing(
            &db,
            "digest",
            "# 已生成的正文",
            &[json!({"paper":works[0]})],
            &works,
            true,
        )
        .await
        .unwrap();
        // This is also the state retained if optional archival later fails.
        let item: Briefing = sqlx::query_as("SELECT * FROM daily_briefings WHERE id='digest'")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(item.status, "completed");
        assert_eq!(item.markdown, "# 已生成的正文");
        assert_eq!(item.mail_status, "pending");
        assert!(item.conversation_id.is_none());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM briefing_seen")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 2);
    }
}

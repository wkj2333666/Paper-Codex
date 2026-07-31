use crate::{
    acquisition::Acquirer,
    codex::CodexEvent,
    codex_tools::{DynamicToolCall, DynamicToolDefinition, DynamicToolHandler, DynamicToolSession},
    extraction::extract_pdf,
    research::{
        CandidateStatus, DiscoveredWork, EvidenceLevel, ProjectCandidate, ResearchProvider,
        ResearchQuery, ResearchTrigger, SearchRunState,
    },
    research_store::ResearchStore,
    tasks::{IngestInput, TaskEngine},
    workspace::atomic_write,
};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::{mpsc, watch, Mutex, Semaphore};

#[derive(Debug, Clone)]
pub struct ResearchServiceConfig {
    pub cache_dir: PathBuf,
    pub cache_max_bytes: u64,
    pub cache_ttl: Duration,
    pub max_concurrency: usize,
}

#[derive(Clone)]
pub struct ResearchService {
    store: ResearchStore,
    providers: Arc<Vec<Arc<dyn ResearchProvider>>>,
    acquirer: Acquirer,
    cache_dir: Arc<PathBuf>,
    cache_max_bytes: u64,
    cache_ttl: Duration,
    semaphore: Arc<Semaphore>,
}

#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub project_id: String,
    pub conversation_id: String,
    pub message_id: String,
    pub trigger: ResearchTrigger,
    pub question: String,
    pub query: ResearchQuery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderState {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub state: ProviderState,
    pub hits: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchOutcome {
    pub run_id: String,
    pub state: SearchRunState,
    pub works: Vec<DiscoveredWork>,
    pub providers: HashMap<String, ProviderStatus>,
}

#[derive(Debug, thiserror::Error)]
#[error("literature search {run_id} failed: {message}")]
pub struct ResearchSearchError {
    pub run_id: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct InspectionOutcome {
    pub work: DiscoveredWork,
    pub evidence_level: EvidenceLevel,
    pub text: String,
    pub source_url: String,
    pub pdf_path: Option<PathBuf>,
    pub markdown_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ImportCandidateOutcome {
    AlreadyInProject { paper_id: String },
    LinkedExisting { paper_id: String },
    Enqueued { task_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PruneOutcome {
    pub removed_files: usize,
    pub removed_bytes: u64,
    pub remaining_bytes: u64,
}

pub struct ProjectResearchToolHandler {
    research: Arc<ResearchService>,
    project_id: String,
    conversation_id: String,
    message_id: String,
    trigger: ResearchTrigger,
    evidence: Arc<Mutex<HashMap<String, crate::research::CandidateSource>>>,
    last_run_id: Mutex<Option<String>>,
    search_attempted: AtomicBool,
    cancel: watch::Receiver<bool>,
    events: mpsc::UnboundedSender<CodexEvent>,
}

impl ProjectResearchToolHandler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        research: Arc<ResearchService>,
        project_id: String,
        conversation_id: String,
        message_id: String,
        trigger: ResearchTrigger,
        cancel: watch::Receiver<bool>,
        events: mpsc::UnboundedSender<CodexEvent>,
    ) -> Self {
        Self {
            research,
            project_id,
            conversation_id,
            message_id,
            trigger,
            evidence: Arc::new(Mutex::new(HashMap::new())),
            last_run_id: Mutex::new(None),
            search_attempted: AtomicBool::new(false),
            cancel,
            events,
        }
    }

    pub fn definitions() -> Vec<DynamicToolDefinition> {
        vec![
            DynamicToolDefinition {
                name: "research_search".into(),
                description: "检索与当前项目问题相关的外部学术论文；返回候选 work_id。".into(),
                input_schema: serde_json::json!({
                    "type":"object",
                    "additionalProperties":false,
                    "required":["query","reason"],
                    "properties":{
                        "query":{"type":"string"},
                        "reason":{"type":"string"},
                        "title_terms":{"type":"array","items":{"type":"string"},"default":[]},
                        "author":{"type":["string","null"],"default":null},
                        "year_from":{"type":["integer","null"],"default":null},
                        "year_to":{"type":["integer","null"],"default":null},
                        "limit":{"type":"integer","minimum":1,"maximum":50,"default":10}
                    }
                }),
            },
            DynamicToolDefinition {
                name: "research_inspect".into(),
                description: "查证本轮检索命中或当前项目候选论文的摘要/全文证据。".into(),
                input_schema: serde_json::json!({
                    "type":"object",
                    "additionalProperties":false,
                    "required":["work_id"],
                    "properties":{
                        "work_id":{"type":"string"},
                        "prefer_fulltext":{"type":"boolean","default":false}
                    }
                }),
            },
            DynamicToolDefinition {
                name: "research_save".into(),
                description: "把经判断确实相关的检索结果保存为当前项目候选论文。".into(),
                input_schema: serde_json::json!({
                    "type":"object",
                    "additionalProperties":false,
                    "required":["work_id","reason"],
                    "properties":{
                        "work_id":{"type":"string"},
                        "reason":{"type":"string"},
                        "tags":{"type":"array","items":{"type":"string"},"default":[]}
                    }
                }),
            },
        ]
    }

    pub fn session(self: &Arc<Self>) -> DynamicToolSession {
        DynamicToolSession {
            definitions: Self::definitions(),
            handler: self.clone(),
        }
    }

    pub fn search_attempted(&self) -> bool {
        self.search_attempted.load(Ordering::Relaxed)
    }

    pub async fn evidence(&self) -> HashMap<String, crate::research::CandidateSource> {
        self.evidence.lock().await.clone()
    }

    async fn require_research_scope(&self) -> Result<()> {
        let scopes = self
            .research
            .store()
            .database()
            .conversation_scopes(&self.conversation_id)
            .await?;
        let projects = scopes
            .iter()
            .filter(|scope| scope.scope_type == "project")
            .filter_map(|scope| scope.scope_id.as_deref())
            .collect::<Vec<_>>();
        let allowed = projects.len() == 1 && projects[0] == self.project_id;
        if !allowed {
            bail!("研究工具只允许用于唯一且匹配的项目作用域");
        }
        Ok(())
    }

    async fn require_available_work(&self, work_id: &str) -> Result<()> {
        if !self
            .research
            .store()
            .work_available_to_message(&self.project_id, &self.message_id, work_id)
            .await?
        {
            bail!("当前项目和本轮检索中不存在该候选论文");
        }
        Ok(())
    }

    fn progress(&self, kind: &str, payload: Value) {
        let _ = self.events.send(CodexEvent {
            kind: kind.into(),
            text: None,
            payload,
        });
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResearchSearchArguments {
    query: String,
    reason: String,
    #[serde(default)]
    title_terms: Vec<String>,
    author: Option<String>,
    year_from: Option<i64>,
    year_to: Option<i64>,
    #[serde(default = "default_research_limit")]
    limit: usize,
}

fn default_research_limit() -> usize {
    10
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResearchInspectArguments {
    work_id: String,
    #[serde(default)]
    prefer_fulltext: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResearchSaveArguments {
    work_id: String,
    reason: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[async_trait]
impl DynamicToolHandler for ProjectResearchToolHandler {
    async fn call(&self, call: DynamicToolCall) -> Result<Vec<Value>> {
        self.require_research_scope().await?;
        match call.tool.as_str() {
            "research_search" => {
                let arguments: ResearchSearchArguments =
                    serde_json::from_value(call.arguments).context("研究检索参数无效")?;
                self.search_attempted.store(true, Ordering::Relaxed);
                self.progress(
                    "research-searching",
                    serde_json::json!({"query":arguments.query}),
                );
                let outcome = self
                    .research
                    .search_with_cancel(
                        SearchRequest {
                            project_id: self.project_id.clone(),
                            conversation_id: self.conversation_id.clone(),
                            message_id: self.message_id.clone(),
                            trigger: self.trigger,
                            question: arguments.reason,
                            query: ResearchQuery {
                                text: arguments.query,
                                title_terms: arguments.title_terms,
                                author: arguments.author,
                                year_from: arguments.year_from,
                                year_to: arguments.year_to,
                                limit: arguments.limit,
                            },
                        },
                        self.cancel.clone(),
                    )
                    .await?;
                *self.last_run_id.lock().await = Some(outcome.run_id.clone());
                self.progress(
                    "research-deduplicating",
                    serde_json::json!({"run_id":outcome.run_id,"works":outcome.works.len()}),
                );
                if outcome.state == SearchRunState::Partial {
                    self.progress(
                        "research-partial",
                        serde_json::json!({"run_id":outcome.run_id,"providers":outcome.providers}),
                    );
                }
                Ok(vec![serde_json::json!({
                    "run_id":outcome.run_id,
                    "state":outcome.state,
                    "providers":outcome.providers,
                    "works":outcome.works,
                })])
            }
            "research_inspect" => {
                let arguments: ResearchInspectArguments =
                    serde_json::from_value(call.arguments).context("候选查证参数无效")?;
                self.require_available_work(&arguments.work_id).await?;
                self.progress(
                    if arguments.prefer_fulltext {
                        "research-fetching-fulltext"
                    } else {
                        "research-inspecting-abstract"
                    },
                    serde_json::json!({"work_id":arguments.work_id}),
                );
                let inspection = self
                    .research
                    .inspect(&arguments.work_id, arguments.prefer_fulltext)
                    .await?;
                let source = crate::research::CandidateSource {
                    work_id: inspection.work.id.clone(),
                    title: inspection.work.metadata.title.clone(),
                    source_url: inspection.source_url.clone(),
                    evidence_level: inspection.evidence_level,
                    abstract_text: inspection.work.metadata.abstract_text.clone(),
                    pdf_url: inspection.work.metadata.pdf_url.clone(),
                };
                self.evidence
                    .lock()
                    .await
                    .insert(source.work_id.clone(), source.clone());
                Ok(vec![serde_json::json!({
                    "source":source,
                    "text":inspection.text,
                })])
            }
            "research_save" => {
                let arguments: ResearchSaveArguments =
                    serde_json::from_value(call.arguments).context("候选保存参数无效")?;
                self.require_available_work(&arguments.work_id).await?;
                self.progress(
                    "research-saving-candidates",
                    serde_json::json!({"work_id":arguments.work_id}),
                );
                let run_id = self.last_run_id.lock().await.clone();
                let candidate = self
                    .research
                    .save_candidate(
                        &self.project_id,
                        &arguments.work_id,
                        &arguments.reason,
                        &arguments.tags,
                        run_id.as_deref(),
                        Some(&self.conversation_id),
                    )
                    .await?;
                Ok(vec![serde_json::to_value(candidate)?])
            }
            _ => bail!("未注册的项目研究工具"),
        }
    }
}

impl ResearchService {
    pub fn new(
        store: ResearchStore,
        providers: Vec<Arc<dyn ResearchProvider>>,
        acquirer: Acquirer,
        config: ResearchServiceConfig,
    ) -> Result<Self> {
        if config.max_concurrency == 0 {
            bail!("research concurrency must be greater than zero");
        }
        Ok(Self {
            store,
            providers: Arc::new(providers),
            acquirer,
            cache_dir: Arc::new(config.cache_dir),
            cache_max_bytes: config.cache_max_bytes,
            cache_ttl: config.cache_ttl,
            semaphore: Arc::new(Semaphore::new(config.max_concurrency)),
        })
    }

    pub fn store(&self) -> &ResearchStore {
        &self.store
    }

    pub async fn search(&self, request: SearchRequest) -> Result<SearchOutcome> {
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        self.search_with_cancel(request, cancel_rx).await
    }

    pub async fn search_with_cancel(
        &self,
        request: SearchRequest,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<SearchOutcome> {
        let normalized_query = request.query.normalized()?;
        let run = self
            .store
            .start_search(
                &request.project_id,
                &request.conversation_id,
                &request.message_id,
                request.trigger,
                &request.question,
            )
            .await?;
        if *cancel.borrow() {
            self.store
                .finish_search(
                    &run.id,
                    SearchRunState::Cancelled,
                    &serde_json::json!({}),
                    Some("cancelled"),
                )
                .await?;
            return Ok(SearchOutcome {
                run_id: run.id,
                state: SearchRunState::Cancelled,
                works: Vec::new(),
                providers: HashMap::new(),
            });
        }

        let searches = self.providers.iter().map(|provider| {
            let provider = provider.clone();
            let query = normalized_query.clone();
            let semaphore = self.semaphore.clone();
            async move {
                let name = provider.name().to_owned();
                let permit = semaphore
                    .acquire_owned()
                    .await
                    .context("research semaphore closed");
                let result = match permit {
                    Ok(_permit) => provider.search(&query).await,
                    Err(error) => Err(error),
                };
                (name, result)
            }
        });
        let joined = join_all(searches);
        tokio::pin!(joined);
        let results = tokio::select! {
            _ = wait_for_cancel(&mut cancel) => {
                self.store.finish_search(
                    &run.id,
                    SearchRunState::Cancelled,
                    &serde_json::json!({}),
                    Some("cancelled"),
                ).await?;
                return Ok(SearchOutcome {
                    run_id: run.id,
                    state: SearchRunState::Cancelled,
                    works: Vec::new(),
                    providers: HashMap::new(),
                });
            }
            results = &mut joined => results,
        };

        let mut providers = HashMap::new();
        let mut works = BTreeMap::<String, DiscoveredWork>::new();
        let mut failures = Vec::new();
        for (provider, result) in results {
            match result {
                Ok(provider_works) => {
                    let mut persisted = Vec::with_capacity(provider_works.len());
                    for work in provider_works {
                        let work = self.store.upsert_work(work).await?;
                        works.insert(work.id.clone(), work.clone());
                        persisted.push(work);
                    }
                    self.store
                        .save_search_results(&run.id, &provider, &persisted)
                        .await?;
                    providers.insert(
                        provider,
                        ProviderStatus {
                            state: ProviderState::Completed,
                            hits: persisted.len(),
                            error: None,
                        },
                    );
                }
                Err(error) => {
                    let message = error.to_string();
                    failures.push(format!("{provider}: {message}"));
                    providers.insert(
                        provider,
                        ProviderStatus {
                            state: ProviderState::Failed,
                            hits: 0,
                            error: Some(message),
                        },
                    );
                }
            }
        }

        let provider_status = serde_json::to_value(&providers)?;
        if providers.is_empty()
            || providers
                .values()
                .all(|status| status.state == ProviderState::Failed)
        {
            let message = if failures.is_empty() {
                "no research providers are configured".to_owned()
            } else {
                failures.join("; ")
            };
            self.store
                .finish_search(
                    &run.id,
                    SearchRunState::Failed,
                    &provider_status,
                    Some(&message),
                )
                .await?;
            return Err(ResearchSearchError {
                run_id: run.id,
                message,
            }
            .into());
        }
        let state = if failures.is_empty() {
            SearchRunState::Completed
        } else {
            SearchRunState::Partial
        };
        self.store
            .finish_search(&run.id, state, &provider_status, None)
            .await?;
        Ok(SearchOutcome {
            run_id: run.id,
            state,
            works: works.into_values().collect(),
            providers,
        })
    }

    pub async fn inspect(&self, work_id: &str, prefer_fulltext: bool) -> Result<InspectionOutcome> {
        let work = self
            .store
            .get_work(work_id)
            .await?
            .context("discovered work does not exist")?;
        if !prefer_fulltext || work.metadata.pdf_url.is_none() {
            return Ok(abstract_inspection(work));
        }

        let pdf_url = work
            .metadata
            .pdf_url
            .as_deref()
            .context("discovered work has no PDF URL")?;
        let work_dir = self.work_cache_dir(&work.id);
        tokio::fs::create_dir_all(&work_dir).await?;
        let pdf_path = work_dir.join("source.pdf");
        let bytes = match tokio::fs::read(&pdf_path).await {
            Ok(bytes) => {
                self.acquirer.validate_pdf(&bytes)?;
                bytes
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let bytes = self.acquirer.download_pdf(pdf_url).await?;
                atomic_write(&pdf_path, &bytes).await?;
                bytes
            }
            Err(error) => return Err(error.into()),
        };
        let sha256 = hex::encode(Sha256::digest(&bytes));
        let extracted = extract_pdf(&pdf_path, &work_dir, &sha256).await?;
        let markdown_path = work_dir.join("extracted.md");
        atomic_write(&markdown_path, extracted.markdown.as_bytes()).await?;
        let metadata_path = work_dir.join("metadata.json");
        atomic_write(
            &metadata_path,
            &serde_json::to_vec_pretty(&serde_json::json!({
                "work_id": &work.id,
                "sha256": sha256,
                "source_url": &work.metadata.source_url,
                "pdf_url": pdf_url,
                "evidence_level": "fulltext",
            }))?,
        )
        .await?;
        let work = self
            .store
            .set_work_evidence(work_id, EvidenceLevel::Fulltext)
            .await?;
        Ok(InspectionOutcome {
            source_url: work.metadata.source_url.clone(),
            work,
            evidence_level: EvidenceLevel::Fulltext,
            text: extracted.markdown,
            pdf_path: Some(pdf_path),
            markdown_path: Some(markdown_path),
        })
    }

    pub async fn save_candidate(
        &self,
        project_id: &str,
        work_id: &str,
        reason: &str,
        tags: &[String],
        run_id: Option<&str>,
        conversation_id: Option<&str>,
    ) -> Result<ProjectCandidate> {
        self.store
            .save_candidate(project_id, work_id, reason, tags, run_id, conversation_id)
            .await
    }

    pub async fn dismiss_candidate(
        &self,
        project_id: &str,
        work_id: &str,
    ) -> Result<ProjectCandidate> {
        self.store
            .set_candidate_status(project_id, work_id, CandidateStatus::Dismissed)
            .await
    }

    pub async fn remove_candidate(&self, project_id: &str, work_id: &str) -> Result<()> {
        self.store.remove_candidate(project_id, work_id).await
    }

    pub async fn import_candidate(
        &self,
        project_id: &str,
        work_id: &str,
        engine: Option<&TaskEngine>,
    ) -> Result<ImportCandidateOutcome> {
        let candidate = self
            .store
            .get_candidate(project_id, work_id)
            .await?
            .context("project candidate does not exist")?;
        if candidate.status == CandidateStatus::Importing {
            bail!("project candidate is already importing");
        }
        if let Some(paper) = self
            .store
            .database()
            .find_paper_by_identity(
                candidate.work.metadata.doi.as_deref(),
                candidate.work.metadata.arxiv_id.as_deref(),
            )
            .await?
        {
            let project_ids = self.store.database().paper_project_ids(&paper.id).await?;
            self.store
                .database()
                .add_paper_to_project(&paper.id, project_id)
                .await?;
            sqlx::query(
                r#"UPDATE project_candidates
                   SET status='imported',paper_id=?,import_task_id=NULL,
                       updated_at=CURRENT_TIMESTAMP
                   WHERE project_id=? AND work_id=?"#,
            )
            .bind(&paper.id)
            .bind(project_id)
            .bind(work_id)
            .execute(self.store.database().pool())
            .await?;
            return if project_ids.iter().any(|id| id == project_id) {
                Ok(ImportCandidateOutcome::AlreadyInProject { paper_id: paper.id })
            } else {
                Ok(ImportCandidateOutcome::LinkedExisting { paper_id: paper.id })
            };
        }

        let source = candidate
            .work
            .metadata
            .arxiv_id
            .as_deref()
            .map(|id| format!("arxiv:{id}"))
            .or_else(|| {
                candidate
                    .work
                    .metadata
                    .doi
                    .as_deref()
                    .map(|doi| format!("doi:{doi}"))
            })
            .or_else(|| candidate.work.metadata.pdf_url.clone())
            .context("candidate has no importable source")?;
        let engine = engine.context("paper import service is unavailable")?;
        let task_id = engine
            .create_ingest(IngestInput {
                source,
                project_id: Some(project_id.to_owned()),
                upload_path: None,
            })
            .await?;
        if let Err(error) = self
            .store
            .mark_candidate_importing(project_id, work_id, &task_id)
            .await
        {
            let _ = engine.cancel(&task_id).await;
            return Err(error);
        }
        Ok(ImportCandidateOutcome::Enqueued { task_id })
    }

    pub async fn recover_interrupted_runs(&self) -> Result<()> {
        self.store.recover_interrupted_research().await
    }

    pub async fn prune_cache(&self) -> Result<PruneOutcome> {
        let cache_dir = self.cache_dir.as_ref().clone();
        let max_bytes = self.cache_max_bytes;
        let ttl = self.cache_ttl;
        tokio::task::spawn_blocking(move || prune_cache_files(&cache_dir, max_bytes, ttl))
            .await
            .context("join research cache pruning task")?
    }

    fn work_cache_dir(&self, work_id: &str) -> PathBuf {
        let safe_id = work_id
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                    character
                } else {
                    '_'
                }
            })
            .collect::<String>();
        self.cache_dir.join("works").join(safe_id)
    }
}

fn abstract_inspection(work: DiscoveredWork) -> InspectionOutcome {
    let (evidence_level, text) = match work.metadata.abstract_text.clone() {
        Some(abstract_text) => (EvidenceLevel::Abstract, abstract_text),
        None => (EvidenceLevel::Metadata, work.metadata.title.clone()),
    };
    InspectionOutcome {
        source_url: work.metadata.source_url.clone(),
        work,
        evidence_level,
        text,
        pdf_path: None,
        markdown_path: None,
    }
}

async fn wait_for_cancel(cancel: &mut watch::Receiver<bool>) {
    if *cancel.borrow() {
        return;
    }
    while cancel.changed().await.is_ok() {
        if *cancel.borrow() {
            return;
        }
    }
    futures::future::pending::<()>().await;
}

#[derive(Debug)]
struct CacheFile {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
}

fn prune_cache_files(root: &Path, max_bytes: u64, ttl: Duration) -> Result<PruneOutcome> {
    let now = SystemTime::now();
    let mut files = collect_cache_files(root)?;
    let mut remaining_bytes = files.iter().map(|file| file.size).sum::<u64>();
    files.sort_by_key(|file| file.modified);
    let mut removed_files = 0;
    let mut removed_bytes = 0;
    for file in files {
        let expired = now
            .duration_since(file.modified)
            .is_ok_and(|age| age >= ttl);
        if !expired && remaining_bytes <= max_bytes {
            continue;
        }
        match std::fs::remove_file(&file.path) {
            Ok(()) => {
                removed_files += 1;
                removed_bytes += file.size;
                remaining_bytes = remaining_bytes.saturating_sub(file.size);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(PruneOutcome {
        removed_files,
        removed_bytes,
        remaining_bytes,
    })
}

fn collect_cache_files(root: &Path) -> Result<Vec<CacheFile>> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                pending.push(entry.path());
                continue;
            }
            let name = entry.file_name();
            if name != "source.pdf" && name != "extracted.md" {
                continue;
            }
            let metadata = entry.metadata()?;
            files.push(CacheFile {
                path: entry.path(),
                size: metadata.len(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            });
        }
    }
    Ok(files)
}

use crate::{
    acquisition::Acquirer,
    codex::{CodexRuntime, CodexTurn},
    db::Database,
    domain::{Paper, TaskEvent, TaskState},
    extraction::extract_pdf,
    graph::materialize_proposal,
    knowledge::{proposal_schema, KnowledgeRepository, ProposedKnowledge},
    prompts::{first_pass_prompt, scoped_question_prompt},
    search::SearchIndex,
    workspace::{atomic_write, safe_key, Workspace},
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::{broadcast, mpsc, watch, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestInput {
    pub source: String,
    pub project_id: Option<String>,
    pub upload_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionInput {
    pub scope_type: String,
    pub scope_id: Option<String>,
    pub question: String,
}

pub struct TaskEngine {
    db: Database,
    workspace: Workspace,
    acquirer: Acquirer,
    codex: Arc<CodexRuntime>,
    search: SearchIndex,
    knowledge: KnowledgeRepository,
    queue: mpsc::Sender<String>,
    events: broadcast::Sender<TaskEvent>,
    cancellations: Mutex<HashMap<String, watch::Sender<bool>>>,
    active: Mutex<HashSet<String>>,
}

impl TaskEngine {
    pub async fn start(
        db: Database,
        workspace: Workspace,
        acquirer: Acquirer,
        codex: Arc<CodexRuntime>,
    ) -> Result<Arc<Self>> {
        let (queue, mut receiver) = mpsc::channel::<String>(128);
        let (events, _) = broadcast::channel(1024);
        let engine = Arc::new(Self {
            search: SearchIndex::new(db.clone()),
            knowledge: KnowledgeRepository::new(workspace.clone()),
            db,
            workspace,
            acquirer,
            codex,
            queue,
            events,
            cancellations: Mutex::new(HashMap::new()),
            active: Mutex::new(HashSet::new()),
        });
        engine.knowledge.recover().await?;
        for id in engine.db.resumable_task_ids().await? {
            engine.db.reset_task_for_resume(&id).await?;
            engine.queue.send(id).await?;
        }
        let dispatcher = engine.clone();
        tokio::spawn(async move {
            while let Some(id) = receiver.recv().await {
                let run = dispatcher.clone();
                tokio::spawn(async move {
                    run.execute(id).await;
                });
            }
        });
        Ok(engine)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.events.subscribe()
    }

    pub async fn create_ingest(&self, input: IngestInput) -> Result<String> {
        let id = self
            .db
            .create_task("ingest", &serde_json::to_string(&input)?)
            .await?;
        self.emit(&id, "queued", serde_json::json!({"source":input.source}))
            .await?;
        self.queue.send(id.clone()).await?;
        Ok(id)
    }

    pub async fn create_question(&self, input: QuestionInput) -> Result<String> {
        let id = self
            .db
            .create_task("question", &serde_json::to_string(&input)?)
            .await?;
        self.emit(
            &id,
            "queued",
            serde_json::json!({"question":input.question}),
        )
        .await?;
        self.queue.send(id.clone()).await?;
        Ok(id)
    }

    pub async fn cancel(&self, id: &str) -> Result<()> {
        if let Some(sender) = self.cancellations.lock().await.get(id) {
            let _ = sender.send(true);
        }
        let task = self.db.get_task(id).await?.context("task not found")?;
        let state: TaskState = task.state.parse().map_err(anyhow::Error::msg)?;
        if !matches!(
            state,
            TaskState::Done | TaskState::Failed | TaskState::Cancelled
        ) {
            self.db
                .force_task_state(id, TaskState::Cancelled, None)
                .await?;
            self.emit(id, "cancelled", serde_json::json!({})).await?;
        }
        Ok(())
    }

    async fn execute(self: Arc<Self>, id: String) {
        {
            let mut active = self.active.lock().await;
            if !active.insert(id.clone()) {
                return;
            }
        }
        let (cancel_tx, cancel_rx) = watch::channel(false);
        self.cancellations
            .lock()
            .await
            .insert(id.clone(), cancel_tx);
        let result = self.execute_inner(&id, cancel_rx).await;
        if let Err(error) = result {
            let task = self.db.get_task(&id).await.ok().flatten();
            if task.as_ref().is_some_and(|task| task.state != "cancelled") {
                let message = redact_error(&error.to_string());
                let _ = self
                    .db
                    .force_task_state(&id, TaskState::Failed, Some(&message))
                    .await;
                let _ = self
                    .emit(&id, "failed", serde_json::json!({"message":message}))
                    .await;
            }
        }
        self.cancellations.lock().await.remove(&id);
        self.active.lock().await.remove(&id);
    }

    async fn execute_inner(&self, id: &str, cancel: watch::Receiver<bool>) -> Result<()> {
        let task = self.db.get_task(id).await?.context("task disappeared")?;
        match task.kind.as_str() {
            "ingest" => {
                self.ingest(id, serde_json::from_str(&task.input_json)?, cancel)
                    .await
            }
            "question" => {
                self.question(id, serde_json::from_str(&task.input_json)?, cancel)
                    .await
            }
            kind => bail!("unsupported task kind: {kind}"),
        }
    }

    async fn ingest(
        &self,
        id: &str,
        input: IngestInput,
        cancel: watch::Receiver<bool>,
    ) -> Result<()> {
        self.check_cancel(&cancel)?;
        self.stage(id, TaskState::Resolving).await?;
        let resolved = if input.upload_path.is_some() {
            None
        } else {
            Some(self.acquirer.resolve(&input.source).await?)
        };
        self.check_cancel(&cancel)?;
        self.stage(id, TaskState::Fetching).await?;
        let bytes = if let Some(path) = &input.upload_path {
            let bytes = tokio::fs::read(path).await?;
            self.acquirer.validate_pdf(&bytes)?;
            bytes
        } else {
            self.acquirer
                .download_pdf(&resolved.as_ref().unwrap().pdf_url)
                .await?
        };
        let byte_hash = hex::encode(Sha256::digest(&bytes));
        let paper_id = resolved
            .as_ref()
            .and_then(|p| p.identity.clone())
            .unwrap_or_else(|| format!("sha256:{byte_hash}"));
        let title = resolved
            .as_ref()
            .map(|p| p.title.clone())
            .filter(|v| !v.starts_with("arXiv:"))
            .unwrap_or_else(|| input.source.clone());
        let stored = self
            .workspace
            .store_revision(
                &paper_id,
                &bytes,
                resolved.as_ref().map(|p| p.pdf_url.as_str()),
            )
            .await?;
        let now = chrono::Utc::now().to_rfc3339();
        let mut paper = Paper {
            id: paper_id.clone(),
            title,
            authors_json: serde_json::to_string(
                &resolved
                    .as_ref()
                    .map(|p| p.authors.clone())
                    .unwrap_or_default(),
            )?,
            year: resolved.as_ref().and_then(|p| p.year),
            doi: resolved.as_ref().and_then(|p| p.doi.clone()),
            arxiv_id: resolved.as_ref().and_then(|p| p.arxiv_id.clone()),
            canonical_sha256: Some(stored.sha256.clone()),
            source_url: resolved.as_ref().map(|p| p.source_url.clone()),
            note_path: None,
            deleted_at: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.db.upsert_paper(&paper).await?;
        self.db
            .add_revision(
                &paper_id,
                &stored.sha256,
                stored.source_url.as_deref(),
                &stored.artifact_path.to_string_lossy(),
            )
            .await?;
        self.db
            .update_task_context(id, Some(&paper_id), input.project_id.as_deref(), None)
            .await?;
        self.stage(id, TaskState::Extracting).await?;
        let extracted = extract_pdf(
            &stored.artifact_path,
            &self.workspace.state_dir().join("cache"),
            &stored.sha256,
        )
        .await?;
        let staging = self.workspace.staging_dir(id);
        tokio::fs::create_dir_all(&staging).await?;
        let extracted_path = staging.join("extracted.md");
        atomic_write(&extracted_path, extracted.markdown.as_bytes()).await?;
        let metadata_path = staging.join("metadata.json");
        atomic_write(&metadata_path, &serde_json::to_vec_pretty(&resolved)?).await?;
        self.stage(id, TaskState::Analyzing).await?;
        let projects = self.db.list_projects().await?;
        let context = serde_json::to_string_pretty(&projects)?;
        let outcome = self
            .codex
            .run_turn(
                CodexTurn {
                    thread_id: None,
                    cwd: staging.clone(),
                    prompt: first_pass_prompt(&extracted_path, &paper_id, &stored.sha256, &context),
                    output_schema: Some(proposal_schema()),
                    settings: self.codex.default_settings(),
                },
                cancel.clone(),
            )
            .await?;
        if outcome.status != "completed" {
            bail!(
                "Codex turn ended with {}{}",
                outcome.status,
                outcome
                    .error
                    .as_deref()
                    .map(|value| format!(": {value}"))
                    .unwrap_or_default()
            );
        }
        self.db
            .update_task_context(id, None, None, Some(&outcome.thread_id))
            .await?;
        let mut proposal: ProposedKnowledge = serde_json::from_str(&outcome.final_text)
            .context("Codex returned invalid paper knowledge JSON")?;
        proposal.paper.paper_id = paper_id.clone();
        proposal.paper.revision = stored.sha256.clone();
        for evidence in &mut proposal.paper.evidence {
            evidence.paper_id = paper_id.clone();
            evidence.revision = stored.sha256.clone();
        }
        for entity in &mut proposal.entities {
            for evidence in &mut entity.evidence {
                evidence.paper_id = paper_id.clone();
                evidence.revision = stored.sha256.clone();
            }
        }
        for relation in &mut proposal.semantic_relations {
            for evidence in &mut relation.evidence {
                evidence.paper_id = paper_id.clone();
                evidence.revision = stored.sha256.clone();
            }
        }
        if proposal.paper.title.trim().is_empty() {
            proposal.paper.title = paper.title.clone();
        }
        self.stage(id, TaskState::Staging).await?;
        atomic_write(
            &staging.join("codex-output.json"),
            outcome.final_text.as_bytes(),
        )
        .await?;
        self.stage(id, TaskState::Validating).await?;
        self.knowledge.validate(&proposal)?;
        self.stage(id, TaskState::Committing).await?;
        let target = self
            .workspace
            .root()
            .join("library/generated/papers")
            .join(format!("{}.md", safe_key(&paper_id)));
        let base = tokio::fs::read(&target)
            .await
            .ok()
            .map(|v| hex::encode(Sha256::digest(v)));
        let committed = self
            .knowledge
            .commit(id, &proposal, base.as_deref())
            .await?;
        paper.title = proposal.paper.title.clone();
        paper.authors_json = serde_json::to_string(&proposal.paper.authors)?;
        paper.year = proposal.paper.year;
        paper.note_path = Some(committed.note_path.to_string_lossy().to_string());
        self.db.upsert_paper(&paper).await?;
        self.db
            .upsert_paper_analysis(
                &paper_id,
                &stored.sha256,
                &serde_json::to_value(&proposal.paper)?,
            )
            .await?;
        let graph = materialize_proposal(&proposal);
        self.db
            .replace_paper_graph(&paper_id, &stored.sha256, &graph.nodes, &graph.edges)
            .await?;
        if let Some(project_id) = &input.project_id {
            self.db.add_paper_to_project(&paper_id, project_id).await?;
        }
        for slug in &proposal.recommended_projects {
            if let Some(project) = projects.iter().find(|p| &p.slug == slug) {
                self.db.add_paper_to_project(&paper_id, &project.id).await?;
            }
        }
        for relation in &proposal.relations {
            if self
                .db
                .get_paper(&relation.target_paper_id)
                .await?
                .is_some()
            {
                self.db
                    .upsert_relation(
                        &paper_id,
                        &relation.target_paper_id,
                        &relation.relation_type,
                        &serde_json::to_string(&relation.evidence)?,
                        relation.hypothesis,
                    )
                    .await?;
            }
        }
        self.stage(id, TaskState::Indexing).await?;
        let body = tokio::fs::read_to_string(&committed.note_path).await?;
        self.search
            .upsert("paper", &paper_id, &paper.title, &body)
            .await?;
        self.stage(id, TaskState::Done).await?;
        self.emit(id, "result", serde_json::json!({"paper_id":paper_id,"title":paper.title,"note_path":paper.note_path})).await?;
        Ok(())
    }

    async fn question(
        &self,
        id: &str,
        input: QuestionInput,
        cancel: watch::Receiver<bool>,
    ) -> Result<()> {
        self.stage(id, TaskState::Resolving).await?;
        self.stage(id, TaskState::Fetching).await?;
        self.stage(id, TaskState::Extracting).await?;
        self.stage(id, TaskState::Analyzing).await?;
        let context = self
            .context_for(&input.scope_type, input.scope_id.as_deref())
            .await?;
        let staging = self.workspace.staging_dir(id);
        tokio::fs::create_dir_all(&staging).await?;
        atomic_write(&staging.join("context.md"), context.as_bytes()).await?;
        let outcome = self
            .codex
            .run_turn(
                CodexTurn {
                    thread_id: None,
                    cwd: staging,
                    prompt: scoped_question_prompt(&input.scope_type, &input.question, &context),
                    output_schema: None,
                    settings: self.codex.default_settings(),
                },
                cancel,
            )
            .await?;
        if outcome.status != "completed" {
            bail!(
                "Codex question ended with {}{}",
                outcome.status,
                outcome
                    .error
                    .as_deref()
                    .map(|value| format!(": {value}"))
                    .unwrap_or_default()
            );
        }
        self.db
            .update_task_context(
                id,
                None,
                input.scope_id.as_deref(),
                Some(&outcome.thread_id),
            )
            .await?;
        for state in [
            TaskState::Staging,
            TaskState::Validating,
            TaskState::Committing,
            TaskState::Indexing,
            TaskState::Done,
        ] {
            self.stage(id, state).await?;
        }
        self.emit(id, "answer", serde_json::json!({"text":outcome.final_text,"scope":input.scope_type,"scope_id":input.scope_id})).await?;
        Ok(())
    }

    async fn context_for(&self, scope: &str, id: Option<&str>) -> Result<String> {
        let ids = match (scope, id) {
            ("paper", Some(id)) => vec![id.to_owned()],
            ("project", Some(id)) => self.db.project_paper_ids(id).await?,
            _ => self
                .db
                .list_papers()
                .await?
                .into_iter()
                .take(50)
                .map(|p| p.id)
                .collect(),
        };
        let mut context = String::new();
        for id in ids {
            if let Some(paper) = self.db.get_paper(&id).await? {
                if let Some(path) = paper.note_path {
                    if let Ok(note) = tokio::fs::read_to_string(path).await {
                        context
                            .push_str(&format!("\n\n# {} ({})\n{}", paper.title, paper.id, note));
                    }
                }
            }
        }
        Ok(context)
    }

    fn check_cancel(&self, cancel: &watch::Receiver<bool>) -> Result<()> {
        if *cancel.borrow() {
            bail!("task cancelled");
        }
        Ok(())
    }

    async fn stage(&self, id: &str, state: TaskState) -> Result<()> {
        self.db.transition_task(id, state, None).await?;
        self.emit(id, "stage", serde_json::json!({"state":state.as_str()}))
            .await?;
        Ok(())
    }

    async fn emit(&self, id: &str, kind: &str, payload: serde_json::Value) -> Result<()> {
        let event = self
            .db
            .append_event(id, kind, &serde_json::to_string(&payload)?)
            .await?;
        let _ = self.events.send(event);
        Ok(())
    }
}

fn redact_error(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("token") || lower.contains("authorization") || lower.contains("secret") {
        "operation failed; inspect protected service logs".into()
    } else {
        value.chars().take(500).collect()
    }
}

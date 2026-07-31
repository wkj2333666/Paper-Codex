use crate::{
    codex::{
        CodexCapabilities, CodexEvent, CodexIntegrations, CodexRunSettings, CodexRuntime,
        CodexSkillSelection, CodexTurn,
    },
    conversation_context::ConversationContextBuilder,
    conversations::{
        ChatMessage, Conversation, ConversationEvent, ConversationScope, ConversationScopeInput,
    },
    db::Database,
    prompts::{
        conversation_answer_schema, conversation_question_prompt_with_research,
        validate_conversation_answer_with_candidates, ConversationSource,
    },
    research::{ResearchMode, ResearchTrigger},
    research_service::{ProjectResearchToolHandler, ResearchService},
    workspace::Workspace,
};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, mpsc, watch, Mutex};

pub struct ConversationEngine {
    pub db: Database,
    contexts: ConversationContextBuilder,
    codex: Arc<CodexRuntime>,
    research: Option<Arc<ResearchService>>,
    queue: mpsc::Sender<String>,
    events: broadcast::Sender<ConversationEvent>,
    cancellations: Mutex<HashMap<String, watch::Sender<bool>>>,
    enqueue_lock: Mutex<()>,
}

fn should_generate_conversation_title(title: &str) -> bool {
    matches!(
        title.trim(),
        "新对话" | "论文对话" | "项目对话" | "研究对话"
    )
}

#[derive(Default)]
struct AnswerPreview {
    raw: String,
    visible: String,
}

impl AnswerPreview {
    fn push(&mut self, delta: &str) -> Option<String> {
        self.raw.push_str(delta);
        let next = extract_json_string_prefix(&self.raw, "answer_markdown")?;
        let previous_len = self.visible.chars().count();
        if next.chars().count() <= previous_len {
            return None;
        }
        let visible_delta = next.chars().skip(previous_len).collect::<String>();
        self.visible = next;
        Some(visible_delta)
    }
}

fn extract_json_string_prefix(raw: &str, field: &str) -> Option<String> {
    let marker = format!("\"{field}\"");
    let start = raw.find(&marker)? + marker.len();
    let bytes = raw.as_bytes();
    let mut index = start;
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    if bytes.get(index) != Some(&b':') {
        return None;
    }
    index += 1;
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    if bytes.get(index) != Some(&b'\"') {
        return None;
    }
    index += 1;
    let mut output = String::new();
    while index < bytes.len() {
        match bytes[index] {
            b'\"' => return Some(output),
            b'\\' => {
                if index + 1 >= bytes.len() {
                    break;
                }
                match bytes[index + 1] {
                    b'\"' => output.push('"'),
                    b'\\' => output.push('\\'),
                    b'/' => output.push('/'),
                    b'b' => output.push('\u{0008}'),
                    b'f' => output.push('\u{000c}'),
                    b'n' => output.push('\n'),
                    b'r' => output.push('\r'),
                    b't' => output.push('\t'),
                    b'u' if index + 6 <= bytes.len() => {
                        let digits = std::str::from_utf8(&bytes[index + 2..index + 6]).ok()?;
                        let code = u16::from_str_radix(digits, 16).ok()?;
                        output.push(char::from_u32(code as u32).unwrap_or('\u{fffd}'));
                        index += 4;
                    }
                    _ => break,
                }
                index += 2;
            }
            _ => {
                let character = raw[index..].chars().next()?;
                output.push(character);
                index += character.len_utf8();
            }
        }
    }
    Some(output)
}

fn codex_progress(event: &CodexEvent) -> Option<(&'static str, &'static str)> {
    match event.kind.as_str() {
        "turn/started" => Some(("reasoning", "Codex 已开始处理问题…")),
        "agent-delta" => Some(("answering", "Codex 正在生成回答…")),
        "item/reasoning/summaryTextDelta" | "item/reasoning/summaryPartAdded" => {
            Some(("reasoning", "Codex 正在整理推理摘要…"))
        }
        "item/started" | "item/completed" => {
            let item_type = event
                .payload
                .pointer("/params/item/type")
                .and_then(Value::as_str)?;
            match item_type {
                "agentMessage" => Some(("answering", "Codex 正在组织回答…")),
                "commandExecution" => Some(("tool", "Codex 正在执行辅助操作…")),
                "mcpToolCall" => Some(("tool", "Codex 正在调用 MCP 工具…")),
                "fileChange" => Some(("tool", "Codex 正在处理工作区文件…")),
                "webSearch" => Some(("tool", "Codex 正在检索资料…")),
                _ => None,
            }
        }
        _ => None,
    }
}

impl ConversationEngine {
    pub async fn start(
        db: Database,
        workspace: Workspace,
        codex: Arc<CodexRuntime>,
    ) -> Result<Arc<Self>> {
        Self::start_with_research(db, workspace, codex, None).await
    }

    pub async fn start_with_research(
        db: Database,
        workspace: Workspace,
        codex: Arc<CodexRuntime>,
        research: Option<Arc<ResearchService>>,
    ) -> Result<Arc<Self>> {
        Self::recover_states(&db).await?;
        let queued = db.queued_assistant_messages().await?;
        let (queue, mut receiver) = mpsc::channel::<String>(128);
        let (events, _) = broadcast::channel(1024);
        let contexts = match research.as_ref() {
            Some(research) => ConversationContextBuilder::new(db.clone(), workspace)
                .with_research_store(research.store().clone()),
            None => ConversationContextBuilder::new(db.clone(), workspace),
        };
        let engine = Arc::new(Self {
            contexts,
            db,
            codex,
            research,
            queue,
            events,
            cancellations: Mutex::new(HashMap::new()),
            enqueue_lock: Mutex::new(()),
        });
        let worker = engine.clone();
        tokio::spawn(async move {
            while let Some(message_id) = receiver.recv().await {
                worker.run_one(message_id).await;
            }
        });
        for message_id in queued {
            engine.queue.send(message_id).await?;
        }
        Ok(engine)
    }

    pub async fn recover_states(db: &Database) -> Result<()> {
        db.recover_conversation_message_states().await
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ConversationEvent> {
        self.events.subscribe()
    }

    pub fn capabilities(&self) -> CodexCapabilities {
        self.codex.capabilities()
    }

    pub async fn integrations(&self, force_reload: bool) -> Result<CodexIntegrations> {
        self.codex
            .integrations(self.contexts.workspace_root(), force_reload)
            .await
    }

    pub fn validate_settings(&self, settings: &CodexRunSettings) -> Result<CodexRunSettings> {
        self.codex.validate_settings(settings)
    }

    pub async fn create_conversation(
        &self,
        title: &str,
        scopes: Vec<ConversationScopeInput>,
    ) -> Result<Conversation> {
        self.create_conversation_with_settings(title, scopes, None)
            .await
    }

    pub async fn create_conversation_with_settings(
        &self,
        title: &str,
        scopes: Vec<ConversationScopeInput>,
        settings: Option<CodexRunSettings>,
    ) -> Result<Conversation> {
        let settings = settings
            .map(|settings| self.validate_settings(&settings))
            .transpose()?
            .unwrap_or_else(|| self.codex.default_settings());
        let conversation = self.db.create_conversation(title).await?;
        if let Err(error) = self
            .db
            .replace_conversation_scopes(&conversation.id, &scopes)
            .await
        {
            sqlx::query("DELETE FROM conversations WHERE id=?")
                .bind(&conversation.id)
                .execute(self.db.pool())
                .await?;
            return Err(error);
        }
        self.db
            .update_conversation_settings(&conversation.id, &settings)
            .await?
            .context("created conversation settings are missing")
    }

    pub async fn enqueue_message(
        &self,
        conversation_id: &str,
        question: &str,
    ) -> Result<ChatMessage> {
        self.enqueue_message_with_research_mode(conversation_id, question, ResearchMode::Auto)
            .await
    }

    pub async fn enqueue_message_with_research_mode(
        &self,
        conversation_id: &str,
        question: &str,
        research_mode: ResearchMode,
    ) -> Result<ChatMessage> {
        self.enqueue_message_with_options(conversation_id, question, research_mode, None)
            .await
    }

    pub async fn enqueue_message_with_options(
        &self,
        conversation_id: &str,
        question: &str,
        research_mode: ResearchMode,
        skill: Option<CodexSkillSelection>,
    ) -> Result<ChatMessage> {
        let question = question.trim();
        if question.is_empty() {
            bail!("question cannot be empty");
        }
        let _guard = self.enqueue_lock.lock().await;
        let conversation = self
            .db
            .get_conversation(conversation_id)
            .await?
            .context("conversation does not exist")?;
        if conversation.archived_at.is_some() {
            bail!("conversation is archived");
        }
        if self
            .db
            .conversation_has_pending_turn(conversation_id)
            .await?
        {
            bail!("conversation is busy");
        }
        let scopes = self.db.conversation_scopes(conversation_id).await?;
        if scopes.is_empty() {
            bail!("conversation has no context scope");
        }
        if research_mode == ResearchMode::Explicit {
            if research_project_id(&self.db, &scopes).await?.is_none() {
                bail!("显式文献检索需要唯一的项目作用域，或唯一归属于一个项目的论文作用域");
            }
            if self.research.is_none() {
                bail!("项目文献检索服务不可用");
            }
            if !self.codex.capabilities().supports_dynamic_tools {
                bail!("当前 Codex 不支持项目文献检索工具");
            }
        }
        let validated_skill = match skill {
            Some(selection) => {
                let skill = self
                    .codex
                    .validate_skill(self.contexts.workspace_root(), &selection)
                    .await?;
                Some(CodexSkillSelection {
                    name: skill.name,
                    path: skill.path,
                })
            }
            None => None,
        };
        let user = self
            .db
            .append_chat_message_with_research_mode(
                conversation_id,
                "user",
                question,
                "completed",
                research_mode,
                validated_skill.as_ref(),
            )
            .await?;
        let assistant = self
            .db
            .append_chat_message(conversation_id, "assistant", "", "queued")
            .await?;
        self.emit(
            conversation_id,
            Some(&user.id),
            "message-created",
            json!({
                "role":"user",
                "content":question,
                "skill":validated_skill.as_ref().map(|skill| json!({"name":skill.name}))
            }),
        )
        .await?;
        self.emit(
            conversation_id,
            Some(&assistant.id),
            "answer-queued",
            json!({}),
        )
        .await?;
        self.queue.send(assistant.id.clone()).await?;
        Ok(assistant)
    }

    pub async fn cancel(&self, conversation_id: &str) -> Result<()> {
        if let Some(sender) = self.cancellations.lock().await.get(conversation_id) {
            let _ = sender.send(true);
            return Ok(());
        }
        let queued: Vec<String> = sqlx::query_scalar("SELECT id FROM chat_messages WHERE conversation_id=? AND role='assistant' AND status='queued'")
            .bind(conversation_id)
            .fetch_all(self.db.pool())
            .await?;
        for id in queued {
            self.db
                .set_message_result(&id, "", None, "cancelled", Some("用户取消"))
                .await?;
            self.emit(conversation_id, Some(&id), "answer-cancelled", json!({}))
                .await?;
        }
        Ok(())
    }

    async fn run_one(self: &Arc<Self>, message_id: String) {
        let message = match self.db.get_chat_message(&message_id).await {
            Ok(Some(message)) if message.status == "queued" => message,
            _ => return,
        };
        let conversation_id = message.conversation_id.clone();
        let (cancel_tx, cancel_rx) = watch::channel(false);
        self.cancellations
            .lock()
            .await
            .insert(conversation_id.clone(), cancel_tx);
        let result = self.execute_turn(&message, cancel_rx).await;
        if let Err(error) = result {
            let current = self.db.message_status(&message.id).await.ok();
            if current
                .as_deref()
                .is_some_and(|status| !matches!(status, "cancelled" | "completed" | "interrupted"))
            {
                let error_text = error.to_string();
                let _ = self
                    .db
                    .set_message_result(&message.id, "", None, "failed", Some(&error_text))
                    .await;
                let _ = self
                    .emit(
                        &conversation_id,
                        Some(&message.id),
                        "answer-failed",
                        json!({"message":error_text}),
                    )
                    .await;
            }
            let _ = self
                .db
                .set_conversation_runtime(&conversation_id, None, "idle")
                .await;
        }
        self.cancellations.lock().await.remove(&conversation_id);
    }

    async fn execute_turn(
        &self,
        assistant: &ChatMessage,
        cancel: watch::Receiver<bool>,
    ) -> Result<()> {
        let conversation = self
            .db
            .get_conversation(&assistant.conversation_id)
            .await?
            .context("conversation disappeared")?;
        let question = self
            .db
            .previous_user_message(&assistant.id)
            .await?
            .context("assistant message has no user question")?;
        self.db
            .set_message_result(&assistant.id, "", None, "running", None)
            .await?;
        self.db
            .set_conversation_runtime(&conversation.id, None, "running")
            .await?;
        self.emit(
            &conversation.id,
            Some(&assistant.id),
            "answer-started",
            json!({}),
        )
        .await?;
        self.emit(
            &conversation.id,
            Some(&assistant.id),
            "answer-progress",
            json!({"phase":"reading"}),
        )
        .await?;

        let scopes = self.db.conversation_scopes(&conversation.id).await?;
        let bundle = self.contexts.refresh(&conversation.id, &scopes).await?;
        let selected_skill = question
            .skill_name
            .as_ref()
            .zip(question.skill_path.as_ref())
            .map(|(name, path)| CodexSkillSelection {
                name: name.clone(),
                path: path.into(),
            });
        if let Some(skill) = &selected_skill {
            self.emit(
                &conversation.id,
                Some(&assistant.id),
                "answer-progress",
                json!({
                    "phase":"tool",
                    "label":format!("正在使用 Skill：{}", skill.name)
                }),
            )
            .await?;
        }
        self.emit(
            &conversation.id,
            Some(&assistant.id),
            "answer-progress",
            json!({"phase":"reasoning"}),
        )
        .await?;
        let (turn_event_tx, mut turn_event_rx) = mpsc::unbounded_channel();
        let project_id = research_project_id(&self.db, &scopes).await?;
        let research_handler = match (project_id.as_deref(), self.research.as_ref()) {
            (Some(project_id), Some(research))
                if self.codex.capabilities().supports_dynamic_tools =>
            {
                Some(Arc::new(ProjectResearchToolHandler::new(
                    research.clone(),
                    project_id.to_owned(),
                    conversation.id.clone(),
                    question.id.clone(),
                    if question.research_mode == ResearchMode::Explicit {
                        ResearchTrigger::Explicit
                    } else {
                        ResearchTrigger::Automatic
                    },
                    cancel.clone(),
                    turn_event_tx.clone(),
                )))
            }
            _ => None,
        };
        if research_handler.is_some() {
            self.emit(
                &conversation.id,
                Some(&assistant.id),
                "answer-progress",
                json!({
                    "phase":"research-planning",
                    "label":"Codex 正在判断是否需要检索外部论文…"
                }),
            )
            .await?;
        }
        let mut preview = AnswerPreview::default();
        let turn = self.codex.run_turn_with_events_and_tools(
            CodexTurn {
                thread_id: conversation.thread_id.clone(),
                cwd: bundle.root.clone(),
                prompt: conversation_question_prompt_with_research(
                    &question.content,
                    question.research_mode,
                    research_handler.is_some(),
                ),
                skill: selected_skill,
                output_schema: Some(conversation_answer_schema()),
                settings: conversation
                    .model
                    .as_ref()
                    .zip(conversation.reasoning_effort.as_ref())
                    .map(|(model, reasoning_effort)| CodexRunSettings {
                        model: model.clone(),
                        reasoning_effort: reasoning_effort.clone(),
                        service_tier: conversation.service_tier.clone(),
                    })
                    .map(|settings| self.validate_settings(&settings))
                    .transpose()?
                    .unwrap_or_else(|| self.codex.default_settings()),
            },
            cancel,
            turn_event_tx,
            research_handler.as_ref().map(|handler| handler.session()),
        );
        tokio::pin!(turn);
        let outcome = loop {
            tokio::select! {
                result = &mut turn => {
                    let outcome = result?;
                    while let Ok(event) = turn_event_rx.try_recv() {
                        self.handle_turn_event(&conversation.id, &assistant.id, &mut preview, event).await?;
                    }
                    break outcome;
                }
                Some(event) = turn_event_rx.recv() => {
                    self.handle_turn_event(&conversation.id, &assistant.id, &mut preview, event).await?;
                }
            }
        };
        if outcome.status != "completed" {
            let status = if outcome.status == "interrupted" {
                "cancelled"
            } else {
                "failed"
            };
            self.db
                .set_message_result(
                    &assistant.id,
                    "",
                    Some(&outcome.turn_id),
                    status,
                    outcome.error.as_deref(),
                )
                .await?;
            bail!("Codex turn ended with {}", outcome.status);
        }
        if question.research_mode == ResearchMode::Explicit
            && !research_handler
                .as_ref()
                .is_some_and(|handler| handler.search_attempted())
        {
            bail!("显式文献检索未执行检索工具");
        }
        let sources = bundle
            .papers
            .iter()
            .map(|paper| ConversationSource {
                paper_id: paper.paper_id.clone(),
                revision: paper.revision.clone(),
                page_count: paper.page_count,
            })
            .collect::<Vec<_>>();
        let candidate_evidence = match research_handler.as_ref() {
            Some(handler) => handler.evidence().await,
            None => HashMap::new(),
        };
        let answer = validate_conversation_answer_with_candidates(
            outcome
                .answer
                .context("Codex returned no structured answer")?,
            &question.content,
            &sources,
            &candidate_evidence,
        )?;
        let generated_title = should_generate_conversation_title(&conversation.title)
            .then(|| answer.title.clone())
            .flatten();
        let citations = self
            .db
            .persist_conversation_answer(&assistant.id, &answer)
            .await?;
        let candidate_citations = if answer.candidate_citations.is_empty() {
            Vec::new()
        } else {
            let project_id = project_id.context("候选论文引用只能写入唯一的项目作用域")?;
            self.research
                .as_ref()
                .context("项目文献检索服务不可用")?
                .store()
                .persist_message_candidate_citations(
                    &assistant.id,
                    &project_id,
                    &answer.candidate_citations,
                )
                .await?
        };
        if let Some(title) = generated_title.as_deref() {
            self.db
                .update_conversation(&conversation.id, Some(title), None)
                .await?;
        }
        self.db
            .set_message_result(
                &assistant.id,
                &answer.answer_markdown,
                Some(&outcome.turn_id),
                "completed",
                None,
            )
            .await?;
        self.db
            .set_conversation_runtime(&conversation.id, Some(&outcome.thread_id), "idle")
            .await?;
        self.emit(
            &conversation.id,
            Some(&assistant.id),
            "answer-completed",
            json!({
                "answer_markdown":answer.answer_markdown,
                "citations":citations,
                "candidate_citations":candidate_citations,
                "title":generated_title
            }),
        )
        .await?;
        Ok(())
    }

    async fn handle_turn_event(
        &self,
        conversation_id: &str,
        message_id: &str,
        preview: &mut AnswerPreview,
        event: CodexEvent,
    ) -> Result<()> {
        if let Some(label) = research_progress_label(&event.kind) {
            self.emit(
                conversation_id,
                Some(message_id),
                "answer-progress",
                json!({"phase":&event.kind,"label":label,"detail":&event.payload}),
            )
            .await?;
        }
        if let Some((phase, label)) = codex_progress(&event) {
            self.emit(
                conversation_id,
                Some(message_id),
                "answer-progress",
                json!({"phase":phase,"label":label}),
            )
            .await?;
        }
        if event.kind == "agent-delta" {
            if let Some(delta) = event.text.as_deref().and_then(|text| preview.push(text)) {
                self.emit(
                    conversation_id,
                    Some(message_id),
                    "answer-delta",
                    json!({"text":delta,"phase":"answering"}),
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn emit(
        &self,
        conversation_id: &str,
        message_id: Option<&str>,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<ConversationEvent> {
        let event = self
            .db
            .append_conversation_event(conversation_id, message_id, event_type, &payload)
            .await?;
        let _ = self.events.send(event.clone());
        Ok(event)
    }
}

fn exact_project_id(scopes: &[ConversationScope]) -> Option<String> {
    (scopes.len() == 1 && scopes[0].scope_type == "project")
        .then(|| scopes[0].scope_id.clone())
        .flatten()
}

async fn research_project_id(
    db: &Database,
    scopes: &[ConversationScope],
) -> Result<Option<String>> {
    if let Some(project_id) = exact_project_id(scopes) {
        return Ok(Some(project_id));
    }
    if scopes.len() != 1 || scopes[0].scope_type != "paper" {
        return Ok(None);
    }
    let Some(paper_id) = scopes[0].scope_id.as_deref() else {
        return Ok(None);
    };
    let project_ids = db.paper_project_ids(paper_id).await?;
    Ok((project_ids.len() == 1).then(|| project_ids[0].clone()))
}

fn research_progress_label(kind: &str) -> Option<&'static str> {
    match kind {
        "research-planning" => Some("Codex 正在规划论文检索…"),
        "research-searching" => Some("Codex 正在检索外部论文…"),
        "research-deduplicating" => Some("Codex 正在合并与去重候选论文…"),
        "research-inspecting-abstract" => Some("Codex 正在查证候选论文摘要…"),
        "research-fetching-fulltext" => Some("Codex 正在获取并查证论文全文…"),
        "research-saving-candidates" => Some("Codex 正在保存相关候选论文…"),
        "research-partial" => Some("部分检索来源暂不可用，Codex 将继续使用已有结果…"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        extract_json_string_prefix, research_project_id, should_generate_conversation_title,
        AnswerPreview,
    };
    use crate::{conversations::ConversationScope, db::Database};

    #[test]
    fn only_placeholder_titles_are_generated() {
        assert!(should_generate_conversation_title("新对话"));
        assert!(should_generate_conversation_title("论文对话"));
        assert!(!should_generate_conversation_title("我的消融实验问题"));
    }

    #[test]
    fn answer_preview_extracts_only_incremental_markdown_from_json() {
        let mut preview = AnswerPreview::default();
        assert_eq!(
            preview.push(r#"{"answer_markdown":"逐步"#),
            Some("逐步".into())
        );
        assert_eq!(preview.push(r#"回答","#), Some("回答".into()));
        assert_eq!(preview.visible, "逐步回答");
        assert_eq!(extract_json_string_prefix(&preview.raw, "citations"), None);
    }

    #[tokio::test]
    async fn research_project_resolves_only_an_unambiguous_scope() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let first = db.create_project("first", "第一个项目", "").await.unwrap();
        let second = db.create_project("second", "第二个项目", "").await.unwrap();
        db.insert_paper("paper:one", "唯一归属论文").await.unwrap();
        db.insert_paper("paper:many", "多重归属论文").await.unwrap();
        db.add_paper_to_project("paper:one", &first).await.unwrap();
        db.add_paper_to_project("paper:many", &first).await.unwrap();
        db.add_paper_to_project("paper:many", &second)
            .await
            .unwrap();

        let scope = |scope_type: &str, scope_id: &str| ConversationScope {
            conversation_id: "conversation".into(),
            scope_type: scope_type.into(),
            scope_id: Some(scope_id.into()),
            added_at: String::new(),
        };

        assert_eq!(
            research_project_id(
                &db,
                &[scope("project", &second), scope("paper", "paper:many")]
            )
                .await
                .unwrap(),
            Some(second)
        );
        assert_eq!(
            research_project_id(&db, &[scope("paper", "paper:one")])
                .await
                .unwrap(),
            Some(first)
        );
        assert_eq!(
            research_project_id(&db, &[scope("paper", "paper:many")])
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            research_project_id(&db, &[scope("paper", "paper:none")])
                .await
                .unwrap(),
            None
        );
    }

}

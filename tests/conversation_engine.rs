use async_trait::async_trait;
use paper_codex::{
    acquisition::Acquirer,
    codex::{CodexCommand, CodexRunSettings, CodexRuntime},
    codex_tools::{DynamicToolCall, DynamicToolHandler},
    conversation_engine::ConversationEngine,
    conversations::ConversationScopeInput,
    db::Database,
    prompts::{
        validate_conversation_answer_with_candidates, ConversationAnswer,
        ConversationCandidateCitation,
    },
    research::{
        canonical_key, EvidenceLevel, ResearchMode, ResearchProvider, ResearchQuery,
        ResearchTrigger, WorkMetadata,
    },
    research_service::{
        ImportCandidateOutcome, ProjectResearchToolHandler, ResearchService, ResearchServiceConfig,
    },
    research_store::ResearchStore,
    workspace::{atomic_write, Workspace},
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{mpsc, watch};

fn fake_command() -> CodexCommand {
    CodexCommand {
        program: PathBuf::from("python3"),
        args: vec![format!(
            "{}/fixtures/fake-app-server.py",
            env!("CARGO_MANIFEST_DIR")
        )],
        codex_home: None,
        runtime_tmp: None,
    }
}

#[tokio::test]
async fn creates_conversation_with_selected_codex_settings() {
    let (engine, _temp) = harness().await;
    let conversation = engine
        .create_conversation_with_settings(
            "高强度分析",
            vec![ConversationScopeInput {
                scope_type: "paper".into(),
                scope_id: Some("paper:one".into()),
            }],
            Some(CodexRunSettings {
                model: "gpt-test".into(),
                reasoning_effort: "high".into(),
                service_tier: Some("priority".into()),
            }),
        )
        .await
        .unwrap();
    assert_eq!(conversation.model.as_deref(), Some("gpt-test"));
    assert_eq!(conversation.reasoning_effort.as_deref(), Some("high"));
    assert_eq!(conversation.service_tier.as_deref(), Some("priority"));
}

#[tokio::test]
async fn new_conversations_default_to_the_research_quality_model() {
    let (engine, _temp) = harness().await;
    let conversation = engine
        .create_conversation(
            "默认研究模型",
            vec![ConversationScopeInput {
                scope_type: "paper".into(),
                scope_id: Some("paper:one".into()),
            }],
        )
        .await
        .unwrap();

    assert_eq!(conversation.model.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(conversation.reasoning_effort.as_deref(), Some("high"));
    assert_eq!(conversation.service_tier, None);
}

async fn harness() -> (Arc<ConversationEngine>, tempfile::TempDir) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.create_project("default", "默认项目", "").await.unwrap();
    db.insert_paper("paper:one", "第一篇论文").await.unwrap();
    sqlx::query("UPDATE papers SET canonical_sha256='revision-one' WHERE id='paper:one'")
        .execute(db.pool())
        .await
        .unwrap();
    let pages = workspace
        .state_dir()
        .join("cache/extraction/revision-one/pages.md");
    atomic_write(&pages, b"<!-- page:1 -->\nevidence")
        .await
        .unwrap();
    let codex = CodexRuntime::spawn(fake_command()).await.unwrap();
    let engine = ConversationEngine::start(db, workspace, codex)
        .await
        .unwrap();
    (engine, temp)
}

async fn wait_done(db: &Database, message_id: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let message = db.get_chat_message(message_id).await.unwrap().unwrap();
            if matches!(
                message.status.as_str(),
                "completed" | "failed" | "cancelled" | "interrupted"
            ) {
                assert_eq!(
                    message.status,
                    "completed",
                    "{}",
                    message.error.unwrap_or_default()
                );
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

async fn next_turn_params(
    mut events: tokio::sync::broadcast::Receiver<paper_codex::codex::CodexEvent>,
) -> serde_json::Value {
    loop {
        let event = events.recv().await.unwrap();
        if event.kind == "test/turn-params" {
            return event.payload["params"].clone();
        }
    }
}

async fn next_thread_params(
    mut events: tokio::sync::broadcast::Receiver<paper_codex::codex::CodexEvent>,
) -> serde_json::Value {
    loop {
        let event = events.recv().await.unwrap();
        if event.kind == "test/thread-params" {
            return event.payload["params"].clone();
        }
    }
}

#[tokio::test]
async fn runs_fifo_and_resumes_the_same_codex_thread() {
    let (engine, _temp) = harness().await;
    let conversation = engine
        .create_conversation(
            "消融",
            vec![ConversationScopeInput {
                scope_type: "paper".into(),
                scope_id: Some("paper:one".into()),
            }],
        )
        .await
        .unwrap();
    let first = engine
        .enqueue_message(&conversation.id, "第一问")
        .await
        .unwrap();
    wait_done(&engine.db, &first.id).await;
    let second = engine
        .enqueue_message(&conversation.id, "第二问")
        .await
        .unwrap();
    wait_done(&engine.db, &second.id).await;

    let stored = engine
        .db
        .get_conversation(&conversation.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.thread_id.as_deref(), Some("thread-fake"));
    assert_eq!(
        engine.db.message_citations(&second.id).await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn marks_running_messages_interrupted_but_leaves_queued_messages_queued() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let conversation = db.create_conversation("恢复测试").await.unwrap();
    let running = db
        .append_chat_message(&conversation.id, "assistant", "", "running")
        .await
        .unwrap();
    let queued = db
        .append_chat_message(&conversation.id, "assistant", "", "queued")
        .await
        .unwrap();

    ConversationEngine::recover_states(&db).await.unwrap();

    assert_eq!(db.message_status(&running.id).await.unwrap(), "interrupted");
    assert_eq!(db.message_status(&queued.id).await.unwrap(), "queued");
}

#[tokio::test]
async fn rejects_a_second_turn_while_the_first_is_pending() {
    let (engine, _temp) = harness().await;
    let conversation = engine
        .create_conversation(
            "串行",
            vec![ConversationScopeInput {
                scope_type: "paper".into(),
                scope_id: Some("paper:one".into()),
            }],
        )
        .await
        .unwrap();
    let first = engine
        .enqueue_message(&conversation.id, "第一问")
        .await
        .unwrap();
    assert!(engine
        .enqueue_message(&conversation.id, "不应排入的第二问")
        .await
        .is_err());
    wait_done(&engine.db, &first.id).await;
}

#[tokio::test]
async fn rejects_lifecycle_changes_while_a_turn_is_pending() {
    let (engine, _temp) = harness().await;
    let conversation = engine
        .create_conversation(
            "待处理",
            vec![ConversationScopeInput {
                scope_type: "paper".into(),
                scope_id: Some("paper:one".into()),
            }],
        )
        .await
        .unwrap();
    engine
        .db
        .append_chat_message(&conversation.id, "assistant", "", "queued")
        .await
        .unwrap();

    let error = engine
        .archive_conversation(&conversation.id)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("busy"));
    assert!(engine
        .db
        .get_conversation(&conversation.id)
        .await
        .unwrap()
        .unwrap()
        .archived_at
        .is_none());
}

#[tokio::test]
async fn publishes_semantic_progress_and_final_answer() {
    let (engine, _temp) = harness().await;
    let conversation = engine
        .create_conversation(
            "流式回答",
            vec![ConversationScopeInput {
                scope_type: "paper".into(),
                scope_id: Some("paper:one".into()),
            }],
        )
        .await
        .unwrap();
    let mut events = engine.subscribe();
    let message = engine
        .enqueue_message(&conversation.id, "请解释")
        .await
        .unwrap();
    let events = tokio::time::timeout(Duration::from_secs(5), async {
        let mut matching = Vec::new();
        loop {
            let event = events.recv().await.unwrap();
            if event.message_id.as_deref() != Some(&message.id) {
                continue;
            }
            let completed = event.event_type == "answer-completed";
            matching.push(event);
            if completed {
                return matching;
            }
        }
    })
    .await
    .unwrap();
    assert!(events
        .iter()
        .any(|event| event.event_type == "answer-started"));
    assert!(events.iter().any(|event| {
        event.event_type == "answer-progress" && event.payload["phase"] == "reading"
    }));
    assert!(events.iter().any(|event| {
        event.event_type == "answer-progress" && event.payload["phase"] == "reasoning"
    }));
    let deltas = events
        .iter()
        .filter(|event| event.event_type == "answer-delta")
        .collect::<Vec<_>>();
    assert!(!deltas.is_empty());
    assert!(deltas.iter().all(|event| event.payload["text"]
        .as_str()
        .is_some_and(|text| !text.contains('{'))));
    assert_eq!(
        deltas
            .iter()
            .filter_map(|event| event.payload["text"].as_str())
            .collect::<String>(),
        "结构化回答 [1]"
    );
    let completed = events
        .iter()
        .find(|event| event.event_type == "answer-completed")
        .unwrap();
    assert_eq!(completed.payload["answer_markdown"], "结构化回答 [1]");
    assert_eq!(completed.payload["citations"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn commentary_agent_messages_become_work_summaries_without_replacing_the_final_answer() {
    let (engine, _temp) = harness().await;
    let conversation = engine
        .create_conversation(
            "多条工作摘要",
            vec![ConversationScopeInput {
                scope_type: "paper".into(),
                scope_id: Some("paper:one".into()),
            }],
        )
        .await
        .unwrap();
    let mut events = engine.subscribe();
    let message = engine
        .enqueue_message(&conversation.id, "multiple-commentary-items")
        .await
        .unwrap();
    let events = tokio::time::timeout(Duration::from_secs(5), async {
        let mut matching = Vec::new();
        loop {
            let event = events.recv().await.unwrap();
            if event.message_id.as_deref() != Some(&message.id) {
                continue;
            }
            let completed = event.event_type == "answer-completed";
            matching.push(event);
            if completed {
                return matching;
            }
        }
    })
    .await
    .unwrap();

    let summaries = events
        .iter()
        .filter(|event| event.event_type == "work-summary-delta")
        .map(|event| {
            (
                event.payload["item_id"].as_str().unwrap(),
                event.payload["text"].as_str().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summaries,
        vec![
            ("commentary-1", "先核验术语"),
            ("commentary-2", "再检查证据"),
        ]
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "answer-delta")
            .filter_map(|event| event.payload["text"].as_str())
            .collect::<String>(),
        "最终回答 [1]"
    );
    let completed = events
        .iter()
        .find(|event| event.event_type == "answer-completed")
        .unwrap();
    assert_eq!(completed.payload["answer_markdown"], "最终回答 [1]");
}

#[derive(Clone)]
struct RuleProvider;

#[async_trait]
impl ResearchProvider for RuleProvider {
    fn name(&self) -> &'static str {
        "fixture"
    }

    async fn search(&self, _query: &ResearchQuery) -> anyhow::Result<Vec<WorkMetadata>> {
        Ok(vec![WorkMetadata {
            canonical_key: canonical_key(Some("10.1000/rules"), None, None).unwrap(),
            doi: Some("10.1000/rules".into()),
            arxiv_id: None,
            openalex_id: None,
            title: "Kolmogorov Complexity of Game Rules".into(),
            authors: vec!["Ada".into()],
            year: Some(2025),
            abstract_text: Some("Rules are represented by shortest descriptions.".into()),
            source_url: "https://doi.org/10.1000/rules".into(),
            pdf_url: None,
            evidence_level: EvidenceLevel::Abstract,
            metadata: serde_json::json!({}),
        }])
    }
}

#[tokio::test]
async fn exact_project_tools_search_inspect_save_and_bind_candidate_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let project = db
        .create_project("rules", "游戏规则", "研究规则复杂度")
        .await
        .unwrap();
    let conversation = db.create_conversation("规则复杂度").await.unwrap();
    db.replace_conversation_scopes(
        &conversation.id,
        &[ConversationScopeInput {
            scope_type: "project".into(),
            scope_id: Some(project.clone()),
        }],
    )
    .await
    .unwrap();
    let message = db
        .append_chat_message_with_research_mode(
            &conversation.id,
            "user",
            "查找相关论文",
            "completed",
            ResearchMode::Explicit,
            None,
        )
        .await
        .unwrap();
    let research = Arc::new(
        ResearchService::new(
            ResearchStore::new(db.clone()),
            vec![Arc::new(RuleProvider)],
            Acquirer::new(1024 * 1024).unwrap(),
            ResearchServiceConfig {
                cache_dir: workspace.root().join(".runtime/research-cache"),
                cache_max_bytes: 1024 * 1024,
                cache_ttl: Duration::from_secs(3600),
                max_concurrency: 1,
            },
        )
        .unwrap(),
    );
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let (events, _event_rx) = mpsc::unbounded_channel();
    let handler = ProjectResearchToolHandler::new(
        research.clone(),
        project.clone(),
        conversation.id.clone(),
        message.id.clone(),
        ResearchTrigger::Explicit,
        cancel_rx,
        events,
    );

    let search = handler
        .call(DynamicToolCall {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            call_id: "search".into(),
            tool: "research_search".into(),
            arguments: serde_json::json!({
                "query": "Kolmogorov game rule complexity",
                "reason": "寻找相关工作",
                "limit": 10
            }),
        })
        .await
        .unwrap();
    let work_id = search[0]["works"][0]["id"].as_str().unwrap().to_owned();
    handler
        .call(DynamicToolCall {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            call_id: "inspect".into(),
            tool: "research_inspect".into(),
            arguments: serde_json::json!({"work_id":work_id,"prefer_fulltext":false}),
        })
        .await
        .unwrap();
    handler
        .call(DynamicToolCall {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            call_id: "save".into(),
            tool: "research_save".into(),
            arguments: serde_json::json!({
                "work_id":work_id,
                "reason":"直接研究规则描述长度",
                "tags":["规则复杂度"]
            }),
        })
        .await
        .unwrap();

    assert!(handler.search_attempted());
    assert_eq!(
        research
            .store()
            .list_project_candidates(&project, false)
            .await
            .unwrap()
            .len(),
        1
    );
    let evidence = handler.evidence().await;
    let answer = validate_conversation_answer_with_candidates(
        ConversationAnswer {
            title: Some("规则复杂度相关工作".into()),
            answer_markdown: "有一篇直接相关工作 [candidate-1]".into(),
            citations: vec![],
            candidate_citations: vec![ConversationCandidateCitation {
                id: "candidate-1".into(),
                work_id: work_id.clone(),
                title: "Kolmogorov Complexity of Game Rules".into(),
                source_url: "https://doi.org/10.1000/rules".into(),
                evidence_level: EvidenceLevel::Abstract,
                quote: "Rules are represented by shortest descriptions.".into(),
                explanation: "直接支持规则描述长度的讨论".into(),
            }],
            annotation_intents: vec![],
        },
        "查找相关论文",
        &[],
        &evidence,
    )
    .unwrap();
    assert_eq!(answer.candidate_citations.len(), 1);

    assert!(research
        .store()
        .database()
        .paper_project_ids("doi:10.1000/rules")
        .await
        .unwrap()
        .is_empty());
    research
        .store()
        .database()
        .insert_paper("doi:10.1000/rules", "Kolmogorov Complexity of Game Rules")
        .await
        .unwrap();
    sqlx::query("UPDATE papers SET doi='10.1000/rules' WHERE id='doi:10.1000/rules'")
        .execute(research.store().database().pool())
        .await
        .unwrap();
    let imported = research
        .import_candidate(&project, &work_id, None)
        .await
        .unwrap();
    assert!(matches!(
        imported,
        ImportCandidateOutcome::LinkedExisting { .. }
    ));
    assert!(research
        .store()
        .database()
        .paper_project_ids("doi:10.1000/rules")
        .await
        .unwrap()
        .contains(&project));
}

#[tokio::test]
async fn explicit_research_tools_reject_non_exact_project_scope() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let project = db.create_project("rules", "规则", "").await.unwrap();
    let conversation = db.create_conversation("论文范围").await.unwrap();
    db.insert_paper("paper:one", "论文").await.unwrap();
    db.replace_conversation_scopes(
        &conversation.id,
        &[ConversationScopeInput {
            scope_type: "paper".into(),
            scope_id: Some("paper:one".into()),
        }],
    )
    .await
    .unwrap();
    let message = db
        .append_chat_message(&conversation.id, "user", "检索论文", "completed")
        .await
        .unwrap();
    let research = Arc::new(
        ResearchService::new(
            ResearchStore::new(db),
            vec![Arc::new(RuleProvider)],
            Acquirer::new(1024 * 1024).unwrap(),
            ResearchServiceConfig {
                cache_dir: workspace.root().join(".runtime/research-cache"),
                cache_max_bytes: 1024,
                cache_ttl: Duration::from_secs(60),
                max_concurrency: 1,
            },
        )
        .unwrap(),
    );
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let (events, _event_rx) = mpsc::unbounded_channel();
    let handler = ProjectResearchToolHandler::new(
        research,
        project,
        conversation.id,
        message.id,
        ResearchTrigger::Explicit,
        cancel_rx,
        events,
    );
    let error = handler
        .call(DynamicToolCall {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            call_id: "search".into(),
            tool: "research_search".into(),
            arguments: serde_json::json!({
                "query":"rules",
                "reason":"scope test",
                "limit":5
            }),
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("项目作用域"));
}

#[tokio::test]
async fn research_tools_accept_a_paper_in_the_handlers_only_project() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let project = db.create_project("rules", "规则", "").await.unwrap();
    let conversation = db.create_conversation("论文范围").await.unwrap();
    db.insert_paper("paper:one", "论文").await.unwrap();
    db.add_paper_to_project("paper:one", &project)
        .await
        .unwrap();
    db.replace_conversation_scopes(
        &conversation.id,
        &[ConversationScopeInput {
            scope_type: "paper".into(),
            scope_id: Some("paper:one".into()),
        }],
    )
    .await
    .unwrap();
    let message = db
        .append_chat_message(&conversation.id, "user", "检索论文", "completed")
        .await
        .unwrap();
    let research = Arc::new(
        ResearchService::new(
            ResearchStore::new(db),
            vec![Arc::new(RuleProvider)],
            Acquirer::new(1024 * 1024).unwrap(),
            ResearchServiceConfig {
                cache_dir: workspace.root().join(".runtime/research-cache"),
                cache_max_bytes: 1024,
                cache_ttl: Duration::from_secs(60),
                max_concurrency: 1,
            },
        )
        .unwrap(),
    );
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let (events, _event_rx) = mpsc::unbounded_channel();
    let handler = ProjectResearchToolHandler::new(
        research,
        project,
        conversation.id,
        message.id,
        ResearchTrigger::Explicit,
        cancel_rx,
        events,
    );

    let search = handler
        .call(DynamicToolCall {
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            call_id: "search".into(),
            tool: "research_search".into(),
            arguments: serde_json::json!({
                "query":"rules",
                "reason":"scope test",
                "limit":5
            }),
        })
        .await
        .unwrap();

    assert_eq!(
        search[0]["works"][0]["title"],
        "Kolmogorov Complexity of Game Rules"
    );
}

#[tokio::test]
async fn outdated_project_thread_is_replaced_with_current_tools_and_bounded_history() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let project = db
        .create_project("infra", "推理基础设施", "比较共享前缀方案")
        .await
        .unwrap();
    db.insert_paper("paper:one", "Hydragen").await.unwrap();
    db.add_paper_to_project("paper:one", &project)
        .await
        .unwrap();
    sqlx::query("UPDATE papers SET canonical_sha256='revision-one' WHERE id='paper:one'")
        .execute(db.pool())
        .await
        .unwrap();
    let pages = workspace
        .state_dir()
        .join("cache/extraction/revision-one/pages.md");
    atomic_write(&pages, b"<!-- page:1 -->\nevidence")
        .await
        .unwrap();
    let conversation = db.create_conversation("共享前缀").await.unwrap();
    db.replace_conversation_scopes(
        &conversation.id,
        &[
            ConversationScopeInput {
                scope_type: "project".into(),
                scope_id: Some(project.clone()),
            },
            ConversationScopeInput {
                scope_type: "paper".into(),
                scope_id: Some("paper:one".into()),
            },
        ],
    )
    .await
    .unwrap();
    db.append_chat_message(
        &conversation.id,
        "user",
        "Hydragen 的关键机制是什么？",
        "completed",
    )
    .await
    .unwrap();
    db.append_chat_message(
        &conversation.id,
        "assistant",
        "它拆分共享前缀与独立后缀注意力。",
        "completed",
    )
    .await
    .unwrap();
    db.set_conversation_runtime(&conversation.id, Some("legacy-thread"), "idle")
        .await
        .unwrap();
    sqlx::query("UPDATE conversations SET dynamic_tools_initialized=1 WHERE id=?")
        .bind(&conversation.id)
        .execute(db.pool())
        .await
        .unwrap();

    let research = Arc::new(
        ResearchService::new(
            ResearchStore::new(db.clone()),
            vec![Arc::new(RuleProvider)],
            Acquirer::new(1024 * 1024).unwrap(),
            ResearchServiceConfig {
                cache_dir: workspace.root().join(".runtime/research-cache"),
                cache_max_bytes: 1024 * 1024,
                cache_ttl: Duration::from_secs(3600),
                max_concurrency: 1,
            },
        )
        .unwrap(),
    );
    let codex = CodexRuntime::spawn(fake_command()).await.unwrap();
    let turn_params = next_turn_params(codex.subscribe());
    let thread_params = next_thread_params(codex.subscribe());
    let engine =
        ConversationEngine::start_with_research(db.clone(), workspace, codex, Some(research))
            .await
            .unwrap();

    let message = engine
        .enqueue_message(
            &conversation.id,
            "observe-thread-params settings 比较 qwen-infra",
        )
        .await
        .unwrap();
    wait_done(&db, &message.id).await;

    let stored = db
        .get_conversation(&conversation.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(stored.thread_id.as_deref(), Some("legacy-thread"));
    let initialized: i64 =
        sqlx::query_scalar("SELECT dynamic_tools_initialized FROM conversations WHERE id=?")
            .bind(&conversation.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(initialized, 1);
    let params = tokio::time::timeout(Duration::from_secs(1), thread_params)
        .await
        .unwrap();
    assert_eq!(params["method"], "thread/start");
    assert!(params["params"]["dynamicTools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "research_import"));
    let params = tokio::time::timeout(Duration::from_secs(1), turn_params)
        .await
        .unwrap();
    let prompt = params["input"][0]["text"].as_str().unwrap();
    assert!(prompt.contains("Hydragen 的关键机制是什么？"));
    assert!(prompt.contains("它拆分共享前缀与独立后缀注意力。"));
}

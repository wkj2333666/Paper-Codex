use paper_codex::{
    codex::{CodexCommand, CodexRunSettings, CodexRuntime},
    conversation_engine::ConversationEngine,
    conversations::ConversationScopeInput,
    db::Database,
    workspace::{atomic_write, Workspace},
};
use std::{path::PathBuf, sync::Arc, time::Duration};

fn fake_command() -> CodexCommand {
    CodexCommand {
        program: PathBuf::from("python3"),
        args: vec![format!(
            "{}/fixtures/fake-app-server.py",
            env!("CARGO_MANIFEST_DIR")
        )],
        codex_home: None,
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

async fn harness() -> (Arc<ConversationEngine>, tempfile::TempDir) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
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

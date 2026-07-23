use paper_codex::{
    conversations::{AnnotationAnchor, ConversationScopeInput},
    db::Database,
    prompts::{ConversationAnswer, ConversationCitation},
};
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use std::{ops::Deref, path::PathBuf};
use uuid::Uuid;

struct TestDatabase {
    database: Database,
    path: PathBuf,
}

impl Deref for TestDatabase {
    type Target = Database;

    fn deref(&self) -> &Self::Target {
        &self.database
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", self.path.display(), suffix));
        }
    }
}

fn database_location() -> (PathBuf, String) {
    let path = std::env::temp_dir().join(format!(
        "paper-codex-conversations-{}.sqlite",
        Uuid::new_v4()
    ));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    (path, url)
}

async fn test_db() -> TestDatabase {
    let (path, url) = database_location();
    let database = Database::connect(&url).await.unwrap();
    TestDatabase { database, path }
}

async fn legacy_database_with_message(
    id: &str,
    scope_type: &str,
    scope_id: &str,
    role: &str,
    content: &str,
) -> TestDatabase {
    let (path, url) = database_location();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE chat_messages (id TEXT PRIMARY KEY, scope_type TEXT NOT NULL, scope_id TEXT, role TEXT NOT NULL, content TEXT NOT NULL, thread_id TEXT, created_at TEXT NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO chat_messages(id,scope_type,scope_id,role,content,thread_id,created_at) VALUES(?,?,?,?,?,NULL,'2020-01-02 03:04:05')",
    )
    .bind(id)
    .bind(scope_type)
    .bind(scope_id)
    .bind(role)
    .bind(content)
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let database = Database::connect(&url).await.unwrap();
    TestDatabase { database, path }
}

fn scope(scope_type: &str, scope_id: Option<&str>) -> ConversationScopeInput {
    ConversationScopeInput {
        scope_type: scope_type.to_owned(),
        scope_id: scope_id.map(str::to_owned),
    }
}

#[tokio::test]
async fn migrates_legacy_chat_messages_without_losing_content() {
    let db = legacy_database_with_message("legacy-1", "paper", "paper:one", "user", "why?").await;
    let messages = db.list_conversation_messages().await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, "legacy-1");
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[0].content, "why?");
    assert_eq!(messages[0].created_at, "2020-01-02 03:04:05");
    assert!(!messages[0].conversation_id.is_empty());

    let scopes = db
        .conversation_scopes(&messages[0].conversation_id)
        .await
        .unwrap();
    assert_eq!(scopes.len(), 1);
    assert_eq!(scopes[0].scope_type, "paper");
    assert_eq!(scopes[0].scope_id.as_deref(), Some("paper:one"));
}

#[tokio::test]
async fn global_scope_is_exclusive_and_specific_scopes_are_deduplicated() {
    let db = test_db().await;
    let conversation = db.create_conversation("研究问题").await.unwrap();
    db.replace_conversation_scopes(
        &conversation.id,
        &[
            scope("paper", Some("paper:one")),
            scope("paper", Some("paper:one")),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        db.conversation_scopes(&conversation.id)
            .await
            .unwrap()
            .len(),
        1
    );

    db.replace_conversation_scopes(&conversation.id, &[scope("global", None)])
        .await
        .unwrap();
    assert_eq!(
        db.conversation_scopes(&conversation.id).await.unwrap()[0].scope_type,
        "global"
    );

    assert!(db
        .replace_conversation_scopes(
            &conversation.id,
            &[scope("global", None), scope("project", Some("project:one"))],
        )
        .await
        .is_err());
}

#[tokio::test]
async fn conversation_crud_separates_active_and_archived_lists() {
    let db = test_db().await;
    let first = db.create_conversation("  消融实验  ").await.unwrap();
    let second = db.create_conversation("动机").await.unwrap();

    assert_eq!(first.title, "消融实验");
    assert_eq!(db.list_conversations(false, 20, 0).await.unwrap().len(), 2);

    let changed = db
        .update_conversation(&first.id, Some("新的标题"), Some(true))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(changed.title, "新的标题");
    assert!(changed.archived_at.is_some());
    assert_eq!(
        db.list_conversations(false, 20, 0).await.unwrap(),
        vec![second]
    );
    assert_eq!(
        db.list_conversations(true, 20, 0).await.unwrap(),
        vec![changed]
    );
}

#[tokio::test]
async fn messages_and_events_are_replayed_once_in_order() {
    let db = test_db().await;
    let conversation = db.create_conversation("方法").await.unwrap();
    let user = db
        .append_chat_message(&conversation.id, "user", "如何设计？", "completed")
        .await
        .unwrap();
    let assistant = db
        .append_chat_message(&conversation.id, "assistant", "", "streaming")
        .await
        .unwrap();
    db.set_message_result(
        &assistant.id,
        "按因素拆分。",
        Some("turn-1"),
        "completed",
        None,
    )
    .await
    .unwrap();

    let first = db
        .append_conversation_event(
            &conversation.id,
            Some(&user.id),
            "message-created",
            &json!({"role":"user"}),
        )
        .await
        .unwrap();
    let second = db
        .append_conversation_event(
            &conversation.id,
            Some(&assistant.id),
            "answer-completed",
            &json!({"turn_id":"turn-1"}),
        )
        .await
        .unwrap();

    let messages = db
        .conversation_messages(&conversation.id, 20, 0)
        .await
        .unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].content, "按因素拆分。");
    assert_eq!(messages[1].turn_id.as_deref(), Some("turn-1"));

    let replay = db
        .conversation_events_after(&conversation.id, first.id)
        .await
        .unwrap();
    assert_eq!(replay, vec![second]);
}

async fn citation_fixture(db: &Database, paper_id: &str, revision: &str) -> String {
    let conversation = db.create_conversation("引用").await.unwrap();
    let message = db
        .append_chat_message(&conversation.id, "assistant", "回答", "completed")
        .await
        .unwrap();
    db.persist_conversation_answer(
        &message.id,
        &ConversationAnswer {
            title: None,
            answer_markdown: "回答".into(),
            citations: vec![ConversationCitation {
                id: "source-1".into(),
                paper_id: paper_id.into(),
                revision: revision.into(),
                page: 2,
                section: Some("Method".into()),
                locator: None,
                quote: "quoted evidence".into(),
                prefix: String::new(),
                suffix: String::new(),
                explanation: "why this matters".into(),
            }],
            annotation_intents: vec![],
        },
    )
    .await
    .unwrap()[0]
        .id
        .clone()
}

#[tokio::test]
async fn citations_can_be_pinned_idempotently_and_listed_with_source_data() {
    let db = test_db().await;
    let citation_id = citation_fixture(&db, "paper:one", "rev-1").await;

    let first = db.pin_citation(&citation_id).await.unwrap().unwrap();
    let second = db.pin_citation(&citation_id).await.unwrap().unwrap();
    assert_eq!(first.id, second.id);

    let annotations = db.paper_annotations("paper:one").await.unwrap();
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].annotation.id, first.id);
    assert_eq!(annotations[0].citation.quote, "quoted evidence");
}

#[tokio::test]
async fn annotation_anchors_are_replaced_atomically() {
    let db = test_db().await;
    let citation_id = citation_fixture(&db, "paper:one", "rev-1").await;
    let annotation = db.pin_citation(&citation_id).await.unwrap().unwrap();
    db.replace_annotation_anchors(
        &annotation.id,
        &[AnnotationAnchor {
            annotation_id: annotation.id.clone(),
            page: 2,
            rect_index: 0,
            x: 0.1,
            y: 0.2,
            width: 0.3,
            height: 0.04,
        }],
    )
    .await
    .unwrap();
    db.replace_annotation_anchors(
        &annotation.id,
        &[AnnotationAnchor {
            annotation_id: annotation.id.clone(),
            page: 2,
            rect_index: 0,
            x: 0.4,
            y: 0.5,
            width: 0.2,
            height: 0.03,
        }],
    )
    .await
    .unwrap();

    let anchors = db.annotation_anchors(&annotation.id).await.unwrap();
    assert_eq!(anchors.len(), 1);
    assert_eq!(anchors[0].x, 0.4);
}

#[tokio::test]
async fn permanent_paper_deletion_retains_annotation_history_as_unavailable() {
    let db = test_db().await;
    db.insert_paper("paper:one", "Paper").await.unwrap();
    let citation_id = citation_fixture(&db, "paper:one", "rev-1").await;
    db.pin_citation(&citation_id).await.unwrap();
    db.trash_paper("paper:one").await.unwrap();
    db.permanently_delete_paper("paper:one").await.unwrap();

    let annotations = db.paper_annotations("paper:one").await.unwrap();
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].annotation.availability, "paper-missing");
}

#[tokio::test]
async fn annotation_availability_tracks_the_current_paper_revision() {
    let db = test_db().await;
    db.insert_paper("paper:one", "Paper").await.unwrap();
    sqlx::query("UPDATE papers SET canonical_sha256='rev-1' WHERE id='paper:one'")
        .execute(db.pool())
        .await
        .unwrap();
    let citation_id = citation_fixture(&db, "paper:one", "rev-1").await;
    db.pin_citation(&citation_id).await.unwrap();
    sqlx::query("UPDATE papers SET canonical_sha256='rev-2' WHERE id='paper:one'")
        .execute(db.pool())
        .await
        .unwrap();

    let annotations = db.paper_annotations("paper:one").await.unwrap();
    assert_eq!(annotations[0].annotation.availability, "revision-stale");
}

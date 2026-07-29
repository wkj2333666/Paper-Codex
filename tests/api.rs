use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use paper_codex::{
    acquisition::Acquirer,
    api::{build_router, AppState},
    auth::Auth,
    codex::{CodexCommand, CodexRuntime},
    conversation_engine::ConversationEngine,
    db::Database,
    domain::Paper,
    prompts::{ConversationAnswer, ConversationCitation},
    research::{CandidateStatus, EvidenceLevel, ResearchTrigger, WorkMetadata},
    research_service::{ResearchService, ResearchServiceConfig},
    research_store::ResearchStore,
    workspace::Workspace,
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tower::ServiceExt;

fn fake_command() -> CodexCommand {
    CodexCommand {
        program: std::path::PathBuf::from("python3"),
        args: vec![format!(
            "{}/fixtures/fake-app-server.py",
            env!("CARGO_MANIFEST_DIR")
        )],
        codex_home: None,
        runtime_tmp: None,
    }
}

fn login_request(password: &str, forwarded_for: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/session")
        .header("content-type", "application/json")
        .header("x-forwarded-for", forwarded_for)
        .body(Body::from(
            serde_json::json!({"password": password}).to_string(),
        ))
        .unwrap()
}

async fn test_app() -> axum::Router {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.keep();
    let workspace = Workspace::initialize(&root).await.unwrap();
    let hash = bcrypt::hash("paper-secret", 4).unwrap();
    build_router(AppState::for_test(
        db,
        workspace,
        Auth::new(hash, "test-jwt-secret".into()),
    ))
}

async fn login_token(app: &axum::Router) -> String {
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"password":"paper-secret"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(login.into_body(), 64 * 1024).await.unwrap();
    let payload: Value = serde_json::from_slice(&bytes).unwrap();
    payload["token"].as_str().unwrap().to_owned()
}

async fn conversation_test_app() -> (axum::Router, Database) {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.keep();
    let workspace = Workspace::initialize(&root).await.unwrap();
    db.insert_paper("paper:one", "第一篇论文").await.unwrap();
    sqlx::query("UPDATE papers SET canonical_sha256='revision-one' WHERE id='paper:one'")
        .execute(db.pool())
        .await
        .unwrap();
    paper_codex::workspace::atomic_write(
        &workspace
            .state_dir()
            .join("cache/extraction/revision-one/pages.md"),
        b"<!-- page:1 -->\nevidence",
    )
    .await
    .unwrap();
    let codex = CodexRuntime::spawn(fake_command()).await.unwrap();
    let conversations = ConversationEngine::start(db.clone(), workspace.clone(), codex)
        .await
        .unwrap();
    let hash = bcrypt::hash("paper-secret", 4).unwrap();
    let state = AppState::for_test(
        db.clone(),
        workspace,
        Auth::new(hash, "test-jwt-secret".into()),
    )
    .with_conversation_engine(conversations);
    (build_router(state), db)
}

async fn task_test_app() -> (axum::Router, Database) {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.keep()).await.unwrap();
    let hash = bcrypt::hash("paper-secret", 4).unwrap();
    let app = build_router(AppState::for_test(
        db.clone(),
        workspace,
        Auth::new(hash, "test-jwt-secret".into()),
    ));
    (app, db)
}

async fn json_response(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 256 * 1024).await.unwrap()).unwrap()
}

#[tokio::test]
async fn task_api_dismisses_terminal_records_and_rejects_running_work() {
    let (app, db) = task_test_app().await;
    let token = login_token(&app).await;
    let failed = db
        .create_task("ingest", r#"{"source":"failed"}"#)
        .await
        .unwrap();
    db.force_task_state(
        &failed,
        paper_codex::domain::TaskState::Failed,
        Some("failed"),
    )
    .await
    .unwrap();
    let running = db
        .create_task("ingest", r#"{"source":"running"}"#)
        .await
        .unwrap();

    let dismissed = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/tasks/{failed}"))
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dismissed.status(), StatusCode::NO_CONTENT);
    assert!(db.get_task(&failed).await.unwrap().is_none());

    let conflict = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/tasks/{running}"))
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);

    let missing = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/tasks/missing")
                .header("x-paper-codex-token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn conversation_api_supports_crud_scopes_and_messages() {
    let (app, db) = conversation_test_app().await;
    let unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"消融","scopes":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let token = login_token(&app).await;
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("x-paper-codex-token", &token)
                .body(Body::from(
                    serde_json::json!({
                        "title":"消融",
                        "scopes":[{"scope_type":"paper","scope_id":"paper:one"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = json_response(created).await;
    let id = created["id"].as_str().unwrap();

    let message = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/conversations/{id}/messages"))
                .header("content-type", "application/json")
                .header("x-paper-codex-token", &token)
                .body(Body::from(r#"{"content":"如何消融？"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(message.status(), StatusCode::ACCEPTED);
    let assistant_id = json_response(message).await["message_id"]
        .as_str()
        .unwrap()
        .to_owned();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if db.message_status(&assistant_id).await.unwrap() == "completed" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();

    let detail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/conversations/{id}"))
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let detail = json_response(detail).await;
    assert_eq!(detail["messages"].as_array().unwrap().len(), 2);
    assert_eq!(
        detail["messages"][1]["citations"].as_array().unwrap().len(),
        1
    );

    let scopes = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/conversations/{id}/scopes"))
                .header("content-type", "application/json")
                .header("x-paper-codex-token", &token)
                .body(Body::from(
                    r#"{"scopes":[{"scope_type":"global","scope_id":null}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(scopes.status(), StatusCode::OK);
    assert_eq!(json_response(scopes).await[0]["scope_type"], "global");

    let legacy = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/questions")
                .header("content-type", "application/json")
                .header("x-paper-codex-token", &token)
                .body(Body::from(
                    r#"{"scope_type":"paper","scope_id":"paper:one","question":"兼容问题"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(legacy.status(), StatusCode::ACCEPTED);
    assert!(json_response(legacy).await["conversation_id"].is_string());

    let archived = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/conversations/{id}"))
                .header("content-type", "application/json")
                .header("x-paper-codex-token", &token)
                .body(Body::from(r#"{"archived":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(archived.status(), StatusCode::OK);
    assert!(json_response(archived).await["archived_at"].is_string());
}

#[tokio::test]
async fn conversation_api_exposes_and_persists_codex_run_settings() {
    let (app, _db) = conversation_test_app().await;
    let token = login_token(&app).await;

    let capabilities = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/codex/capabilities")
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(capabilities.status(), StatusCode::OK);
    let capabilities = json_response(capabilities).await;
    assert_eq!(capabilities["default"]["model"], "gpt-test");
    assert_eq!(capabilities["models"][0]["supports_fast"], true);

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/conversations")
                .header("content-type", "application/json")
                .header("x-paper-codex-token", &token)
                .body(Body::from(
                    serde_json::json!({
                        "title":"运行设置",
                        "scopes":[{"scope_type":"paper","scope_id":"paper:one"}],
                        "settings": {"model":"gpt-test", "reasoning_effort":"high", "service_tier":"priority"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = json_response(created).await;
    assert_eq!(created["reasoning_effort"], "high");
    assert_eq!(created["service_tier"], "priority");
    let id = created["id"].as_str().unwrap();

    let updated = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/conversations/{id}"))
                .header("content-type", "application/json")
                .header("x-paper-codex-token", &token)
                .body(Body::from(
                    serde_json::json!({
                        "settings": {"model":"gpt-test", "reasoning_effort":"low", "service_tier":null}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    let updated = json_response(updated).await;
    assert_eq!(updated["reasoning_effort"], "low");
    assert!(updated["service_tier"].is_null());

    let invalid = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/conversations/{id}"))
                .header("content-type", "application/json")
                .header("x-paper-codex-token", &token)
                .body(Body::from(
                    serde_json::json!({
                        "settings": {"model":"gpt-test", "reasoning_effort":"invalid", "service_tier":null}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn annotation_api_pins_lists_updates_and_stores_anchors() {
    let (app, db) = conversation_test_app().await;
    let conversation = db.create_conversation("批注接口").await.unwrap();
    let message = db
        .append_chat_message(&conversation.id, "assistant", "回答", "completed")
        .await
        .unwrap();
    let citation = db
        .persist_conversation_answer(
            &message.id,
            &ConversationAnswer {
                title: None,
                answer_markdown: "回答".into(),
                citations: vec![ConversationCitation {
                    id: "source-1".into(),
                    paper_id: "paper:one".into(),
                    revision: "revision-one".into(),
                    page: 1,
                    section: Some("Method".into()),
                    locator: None,
                    quote: "evidence".into(),
                    prefix: String::new(),
                    suffix: String::new(),
                    explanation: "说明".into(),
                }],
                candidate_citations: vec![],
                annotation_intents: vec![],
            },
        )
        .await
        .unwrap()
        .remove(0);
    let token = login_token(&app).await;

    let pinned = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/citations/{}/pin", citation.id))
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pinned.status(), StatusCode::OK);
    let annotation = json_response(pinned).await;
    let annotation_id = annotation["id"].as_str().unwrap();

    let anchors = app.clone().oneshot(Request::builder().method("PUT").uri(format!("/api/annotations/{annotation_id}/anchors")).header("content-type", "application/json").header("x-paper-codex-token", &token).body(Body::from(r#"{"anchors":[{"page":1,"rect_index":0,"x":0.1,"y":0.2,"width":0.3,"height":0.04}]}"#)).unwrap()).await.unwrap();
    assert_eq!(anchors.status(), StatusCode::NO_CONTENT);

    let listed = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/paper/annotations?id=paper%3Aone")
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = json_response(listed).await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["anchors"][0]["page"], 1);

    let hidden = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/annotations/{annotation_id}"))
                .header("content-type", "application/json")
                .header("x-paper-codex-token", &token)
                .body(Body::from(r#"{"state":"hidden"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hidden.status(), StatusCode::OK);
    assert_eq!(json_response(hidden).await["state"], "hidden");
}

#[tokio::test]
async fn login_rate_limit_blocks_only_the_failing_forwarded_ip() {
    let app = test_app().await;
    for _ in 0..5 {
        let response = app
            .clone()
            .oneshot(login_request("wrong-password", "198.51.100.10"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let blocked = app
        .clone()
        .oneshot(login_request("paper-secret", "198.51.100.10"))
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(blocked.headers().contains_key("retry-after"));

    let other_client = app
        .oneshot(login_request("paper-secret", "198.51.100.11"))
        .await
        .unwrap();
    assert_eq!(other_client.status(), StatusCode::OK);
}

#[tokio::test]
async fn login_rate_limit_is_cleared_after_success() {
    let app = test_app().await;
    for _ in 0..4 {
        let response = app
            .clone()
            .oneshot(login_request("wrong-password", "203.0.113.20"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let success = app
        .clone()
        .oneshot(login_request("paper-secret", "203.0.113.20"))
        .await
        .unwrap();
    assert_eq!(success.status(), StatusCode::OK);

    let failure_after_success = app
        .clone()
        .oneshot(login_request("wrong-password", "203.0.113.20"))
        .await
        .unwrap();
    assert_eq!(failure_after_success.status(), StatusCode::UNAUTHORIZED);

    let next_success = app
        .oneshot(login_request("paper-secret", "203.0.113.20"))
        .await
        .unwrap();
    assert_eq!(next_success.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_is_public_but_dashboard_requires_login() {
    let app = test_app().await;
    let health = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    let dashboard = app
        .oneshot(
            Request::builder()
                .uri("/api/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dashboard.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_token_in_dedicated_header_authorizes_dashboard_without_exposing_secrets() {
    let app = test_app().await;
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"password":"paper-secret"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let bytes = to_bytes(login.into_body(), 64 * 1024).await.unwrap();
    let payload: Value = serde_json::from_slice(&bytes).unwrap();
    let token = payload["token"].as_str().unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("test-jwt-secret"));
    let dashboard = app
        .oneshot(
            Request::builder()
                .uri("/api/dashboard")
                .header("x-paper-codex-token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dashboard.status(), StatusCode::OK);
}

#[tokio::test]
async fn paper_detail_returns_structured_analysis_without_raw_markdown_frontmatter() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.insert_paper("paper:one", "第一篇论文").await.unwrap();
    db.upsert_paper_analysis(
        "paper:one",
        "rev-1",
        &serde_json::json!({
            "takeaway": "一句话读懂这篇论文。",
            "research_question": "解决什么问题？",
            "method": "核心方法",
            "results": ["关键结果"],
            "limitations": ["主要局限"]
        }),
    )
    .await
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let hash = bcrypt::hash("paper-secret", 4).unwrap();
    let app = build_router(AppState::for_test(
        db,
        workspace,
        Auth::new(hash, "test-jwt-secret".into()),
    ));
    let token = login_token(&app).await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/paper?id=paper%3Aone")
                .header("x-paper-codex-token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
    assert_eq!(body["analysis"]["takeaway"], "一句话读懂这篇论文。");
    assert!(body.get("note").is_none());
}

#[tokio::test]
async fn project_tree_membership_and_paper_trash_are_manageable_through_the_api() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.insert_paper("paper:one", "第一篇论文").await.unwrap();
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let hash = bcrypt::hash("paper-secret", 4).unwrap();
    let app = build_router(AppState::for_test(
        db,
        workspace,
        Auth::new(hash, "test-jwt-secret".into()),
    ));
    let token = login_token(&app).await;

    let create = |body: &'static str| {
        Request::builder()
            .method("POST")
            .uri("/api/projects")
            .header("content-type", "application/json")
            .header("x-paper-codex-token", &token)
            .body(Body::from(body))
            .unwrap()
    };
    let root_response = app
        .clone()
        .oneshot(create(r#"{"name":"根项目"}"#))
        .await
        .unwrap();
    assert_eq!(root_response.status(), StatusCode::CREATED);
    let root: Value = serde_json::from_slice(
        &to_bytes(root_response.into_body(), 64 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    let root_id = root["id"].as_str().unwrap();
    let child_body = serde_json::json!({"name":"子项目","parent_id":root_id}).to_string();
    let child_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/projects")
                .header("content-type", "application/json")
                .header("x-paper-codex-token", &token)
                .body(Body::from(child_body))
                .unwrap(),
        )
        .await
        .unwrap();
    let child: Value = serde_json::from_slice(
        &to_bytes(child_response.into_body(), 64 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    let child_id = child["id"].as_str().unwrap();

    let add_uri = format!("/api/projects/{child_id}/papers/paper%3Aone");
    assert_eq!(
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&add_uri)
                    .header("x-paper-codex-token", &token)
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(&add_uri)
                    .header("x-paper-codex-token", &token)
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );

    let trash = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/paper?id=paper%3Aone")
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(trash.status(), StatusCode::NO_CONTENT);
    let dashboard = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/dashboard")
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let dashboard: Value =
        serde_json::from_slice(&to_bytes(dashboard.into_body(), 64 * 1024).await.unwrap()).unwrap();
    assert_eq!(dashboard["trash_count"], 1);

    assert_eq!(
        app.oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/paper/restore?id=paper%3Aone")
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap()
        )
        .await
        .unwrap()
        .status(),
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn graph_endpoint_backfills_existing_structured_analysis() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.insert_paper("paper:one", "第一篇论文").await.unwrap();
    db.upsert_paper_analysis(
        "paper:one",
        "legacy",
        &serde_json::json!({"method":"注意力方法","results":["结果一"]}),
    )
    .await
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let hash = bcrypt::hash("paper-secret", 4).unwrap();
    let app = build_router(AppState::for_test(
        db,
        workspace,
        Auth::new(hash, "test-jwt-secret".into()),
    ));
    let token = login_token(&app).await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/graph?paper_id=paper%3Aone")
                .header("x-paper-codex-token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let graph: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
    assert_eq!(graph["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(graph["edges"].as_array().unwrap().len(), 2);
    assert!(graph["edges"][0]["hypothesis"].as_bool().unwrap());
}

#[tokio::test]
async fn permanent_delete_removes_generated_and_raw_artifacts_but_not_annotations() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let stored = workspace
        .store_revision("paper:one", b"%PDF-1.4\n%%EOF", None)
        .await
        .unwrap();
    let note = workspace
        .root()
        .join("library/generated/papers/paper_one.md");
    paper_codex::workspace::atomic_write(&note, b"generated note")
        .await
        .unwrap();
    let annotation = workspace.root().join("annotations/papers/paper_one.md");
    paper_codex::workspace::atomic_write(&annotation, b"human note")
        .await
        .unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    db.upsert_paper(&Paper {
        id: "paper:one".into(),
        title: "第一篇论文".into(),
        authors_json: "[]".into(),
        year: None,
        doi: None,
        arxiv_id: None,
        canonical_sha256: Some(stored.sha256.clone()),
        source_url: None,
        note_path: Some(note.to_string_lossy().into()),
        deleted_at: None,
        created_at: now.clone(),
        updated_at: now,
    })
    .await
    .unwrap();
    db.add_revision(
        "paper:one",
        &stored.sha256,
        None,
        &stored.artifact_path.to_string_lossy(),
    )
    .await
    .unwrap();
    db.trash_paper("paper:one").await.unwrap();
    let hash = bcrypt::hash("paper-secret", 4).unwrap();
    let app = build_router(AppState::for_test(
        db,
        workspace,
        Auth::new(hash, "test-jwt-secret".into()),
    ));
    let token = login_token(&app).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/paper/permanent?id=paper%3Aone")
                .header("x-paper-codex-token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(!stored.artifact_path.exists());
    assert!(!note.exists());
    assert!(annotation.exists());
}

#[tokio::test]
async fn paper_pdf_supports_authenticated_ranges_and_etags() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let bytes = b"%PDF-1.4\n0123456789\n%%EOF";
    let stored = workspace
        .store_revision("paper:range", bytes, None)
        .await
        .unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    db.upsert_paper(&Paper {
        id: "paper:range".into(),
        title: "Range".into(),
        authors_json: "[]".into(),
        year: None,
        doi: None,
        arxiv_id: None,
        canonical_sha256: Some(stored.sha256.clone()),
        source_url: None,
        note_path: None,
        deleted_at: None,
        created_at: now.clone(),
        updated_at: now,
    })
    .await
    .unwrap();
    let hash = bcrypt::hash("paper-secret", 4).unwrap();
    let app = build_router(AppState::for_test(
        db,
        workspace,
        Auth::new(hash, "test-jwt-secret".into()),
    ));
    let token = login_token(&app).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/paper/pdf?id=paper%3Arange")
                .header("x-paper-codex-token", &token)
                .header("range", "bytes=0-9")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()["accept-ranges"], "bytes");
    assert_eq!(
        response.headers()["content-range"],
        format!("bytes 0-9/{}", bytes.len())
    );
    let etag = response.headers()["etag"].clone();
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap().len(),
        10
    );
    let cached = app
        .oneshot(
            Request::builder()
                .uri("/api/paper/pdf?id=paper%3Arange")
                .header("x-paper-codex-token", token)
                .header("if-none-match", etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cached.status(), StatusCode::NOT_MODIFIED);
}

async fn research_test_app() -> (axum::Router, Database, Arc<ResearchService>) {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.keep();
    let workspace = Workspace::initialize(&root).await.unwrap();
    let research = Arc::new(
        ResearchService::new(
            ResearchStore::new(db.clone()),
            Vec::new(),
            Acquirer::new(1024 * 1024).unwrap(),
            ResearchServiceConfig {
                cache_dir: root.join(".runtime/research-cache"),
                cache_max_bytes: 1024 * 1024,
                cache_ttl: Duration::from_secs(30 * 24 * 60 * 60),
                max_concurrency: 3,
            },
        )
        .unwrap(),
    );
    let hash = bcrypt::hash("paper-secret", 4).unwrap();
    let state = AppState::for_test(
        db.clone(),
        workspace,
        Auth::new(hash, "test-jwt-secret".into()),
    )
    .with_research_service(research.clone());
    (build_router(state), db, research)
}

fn candidate_work(doi: &str) -> WorkMetadata {
    WorkMetadata {
        canonical_key: format!("doi:{doi}"),
        doi: Some(doi.to_owned()),
        arxiv_id: None,
        openalex_id: None,
        title: "Rule Complexity for Games".to_owned(),
        authors: vec!["Ada Lovelace".to_owned()],
        year: Some(2024),
        abstract_text: Some("Verified abstract".to_owned()),
        source_url: format!("https://doi.org/{doi}"),
        pdf_url: Some("https://example.test/rule.pdf".to_owned()),
        evidence_level: EvidenceLevel::Abstract,
        metadata: serde_json::json!({"fixture": true}),
    }
}

#[tokio::test]
async fn candidate_import_reuses_an_existing_paper_only_after_authenticated_confirmation() {
    let (app, db, research) = research_test_app().await;
    let project = db.create_project("bench", "Bench", "").await.unwrap();
    let work = research
        .store()
        .upsert_work(candidate_work("10.1000/rule"))
        .await
        .unwrap();
    research
        .save_candidate(&project, &work.id, "直接相关", &[], None, None)
        .await
        .unwrap();
    db.insert_paper("doi:10.1000/rule", "Rule Complexity")
        .await
        .unwrap();
    sqlx::query("UPDATE papers SET doi='10.1000/rule' WHERE id='doi:10.1000/rule'")
        .execute(db.pool())
        .await
        .unwrap();

    let unauthenticated = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/projects/{project}/candidates/{}/import",
                    work.id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let token = login_token(&app).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/projects/{project}/candidates/{}/import",
                    work.id
                ))
                .header("x-paper-codex-token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(db
        .paper_project_ids("doi:10.1000/rule")
        .await
        .unwrap()
        .contains(&project));
    assert_eq!(
        research
            .store()
            .get_candidate(&project, &work.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CandidateStatus::Imported
    );
}

#[tokio::test]
async fn candidate_review_distinguishes_dismiss_remove_and_importing_conflict() {
    let (app, db, research) = research_test_app().await;
    let token = login_token(&app).await;
    let project = db.create_project("review", "Review", "").await.unwrap();
    let work = research
        .store()
        .upsert_work(candidate_work("10.1000/review"))
        .await
        .unwrap();
    research
        .save_candidate(&project, &work.id, "值得检查", &[], None, None)
        .await
        .unwrap();

    let dismissed = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/projects/{project}/candidates/{}", work.id))
                .header("content-type", "application/json")
                .header("x-paper-codex-token", &token)
                .body(Body::from(r#"{"status":"dismissed"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dismissed.status(), StatusCode::OK);

    let active = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/projects/{project}/candidates"))
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(json_response(active).await.as_array().unwrap().len(), 0);
    let all = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/projects/{project}/candidates?include_dismissed=true"
                ))
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(json_response(all).await.as_array().unwrap().len(), 1);

    research
        .store()
        .set_candidate_status(&project, &work.id, CandidateStatus::Importing)
        .await
        .unwrap();
    let conflict = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/projects/{project}/candidates/{}", work.id))
                .header("x-paper-codex-token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn project_literature_history_returns_run_results_without_creating_candidates() {
    let (app, db, research) = research_test_app().await;
    let token = login_token(&app).await;
    let project = db.create_project("history", "History", "").await.unwrap();
    let conversation = db.create_conversation("Search").await.unwrap();
    let message = db
        .append_chat_message(&conversation.id, "user", "search", "completed")
        .await
        .unwrap();
    let run = research
        .store()
        .start_search(
            &project,
            &conversation.id,
            &message.id,
            ResearchTrigger::Explicit,
            "rule complexity",
        )
        .await
        .unwrap();
    let work = research
        .store()
        .upsert_work(candidate_work("10.1000/history"))
        .await
        .unwrap();
    research
        .store()
        .save_search_results(&run.id, "crossref", &[work])
        .await
        .unwrap();

    let history = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/projects/{project}/literature-searches"))
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(history.status(), StatusCode::OK);
    assert_eq!(json_response(history).await.as_array().unwrap().len(), 1);

    let detail = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/projects/{project}/literature-searches/{}",
                    run.id
                ))
                .header("x-paper-codex-token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail = json_response(detail).await;
    assert_eq!(detail["results"].as_array().unwrap().len(), 1);
    assert!(research
        .store()
        .list_project_candidates(&project, false)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn project_b_cannot_read_or_mutate_project_a_research_by_guessing_ids() {
    let (app, db, research) = research_test_app().await;
    let token = login_token(&app).await;
    let project_a = db.create_project("a", "Project A", "").await.unwrap();
    let project_b = db.create_project("b", "Project B", "").await.unwrap();
    let conversation = db.create_conversation("Search A").await.unwrap();
    let message = db
        .append_chat_message(&conversation.id, "user", "search", "completed")
        .await
        .unwrap();
    let run = research
        .store()
        .start_search(
            &project_a,
            &conversation.id,
            &message.id,
            ResearchTrigger::Explicit,
            "rule complexity",
        )
        .await
        .unwrap();
    let work = research
        .store()
        .upsert_work(candidate_work("10.1000/private-a"))
        .await
        .unwrap();
    research
        .store()
        .save_search_results(&run.id, "fixture", std::slice::from_ref(&work))
        .await
        .unwrap();
    research
        .save_candidate(&project_a, &work.id, "A 的候选", &[], Some(&run.id), None)
        .await
        .unwrap();

    let b_candidates = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/projects/{project_b}/candidates"))
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(json_response(b_candidates)
        .await
        .as_array()
        .unwrap()
        .is_empty());

    let b_searches = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/projects/{project_b}/literature-searches"))
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(json_response(b_searches)
        .await
        .as_array()
        .unwrap()
        .is_empty());

    let guessed_search = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/projects/{project_b}/literature-searches/{}",
                    run.id
                ))
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(guessed_search.status(), StatusCode::NOT_FOUND);

    let unscoped_search = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/literature-searches/{}", run.id))
                .header("x-paper-codex-token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unscoped_search.status(), StatusCode::NOT_FOUND);

    for request in [
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/projects/{project_b}/candidates/{}", work.id))
            .header("content-type", "application/json")
            .header("x-paper-codex-token", &token)
            .body(Body::from(r#"{"status":"dismissed"}"#))
            .unwrap(),
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/projects/{project_b}/candidates/{}", work.id))
            .header("x-paper-codex-token", &token)
            .body(Body::empty())
            .unwrap(),
        Request::builder()
            .method("POST")
            .uri(format!(
                "/api/projects/{project_b}/candidates/{}/import",
                work.id
            ))
            .header("x-paper-codex-token", &token)
            .body(Body::empty())
            .unwrap(),
    ] {
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn explicit_research_rejects_paper_scope_before_creating_messages() {
    let (app, db) = conversation_test_app().await;
    let token = login_token(&app).await;
    let conversation = db.create_conversation("论文问题").await.unwrap();
    db.replace_conversation_scopes(
        &conversation.id,
        &[paper_codex::conversations::ConversationScopeInput {
            scope_type: "paper".into(),
            scope_id: Some("paper:one".into()),
        }],
    )
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/conversations/{}/messages", conversation.id))
                .header("content-type", "application/json")
                .header("x-paper-codex-token", token)
                .body(Body::from(
                    serde_json::json!({
                        "content":"检索更多相关论文",
                        "research_mode":"explicit"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(db
        .conversation_messages(&conversation.id, 100, 0)
        .await
        .unwrap()
        .is_empty());
}

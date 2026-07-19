use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use paper_codex::{
    api::{build_router, AppState},
    auth::Auth,
    db::Database,
    domain::Paper,
    workspace::Workspace,
};
use serde_json::Value;
use tower::ServiceExt;

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

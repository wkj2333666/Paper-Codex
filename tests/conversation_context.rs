use paper_codex::{
    conversation_context::ConversationContextBuilder,
    conversations::ConversationScope,
    db::Database,
    workspace::{atomic_write, Workspace},
};

fn paper_scope(paper_id: &str) -> ConversationScope {
    ConversationScope {
        conversation_id: "conversation-1".into(),
        scope_type: "paper".into(),
        scope_id: Some(paper_id.into()),
        added_at: "2026-01-01 00:00:00".into(),
    }
}

#[tokio::test]
async fn refreshes_context_atomically_and_reuses_revision_pages() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.insert_paper("paper:one", "第一篇论文").await.unwrap();
    let stored = workspace
        .store_revision("paper:one", b"not-needed-by-context", None)
        .await
        .unwrap();
    sqlx::query("UPDATE papers SET canonical_sha256=? WHERE id='paper:one'")
        .bind(&stored.sha256)
        .execute(db.pool())
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
    let pages = workspace
        .state_dir()
        .join("cache/extraction")
        .join(&stored.sha256)
        .join("pages.md");
    atomic_write(&pages, b"<!-- page:1 -->\nA paper page")
        .await
        .unwrap();

    let stale = workspace
        .state_dir()
        .join("conversations/conversation-1/stale.txt");
    atomic_write(&stale, b"old bundle").await.unwrap();

    let builder = ConversationContextBuilder::new(db, workspace.clone());
    let bundle = builder
        .refresh("conversation-1", &[paper_scope("paper:one")])
        .await
        .unwrap();

    assert!(bundle.manifest_path.is_file());
    assert!(bundle.summary_path.is_file());
    assert!(!bundle.root.join("stale.txt").exists());
    let paper = bundle
        .root
        .join("papers")
        .read_dir()
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(std::fs::read_to_string(paper)
        .unwrap()
        .contains("<!-- page:1 -->"));
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["papers"][0]["paper_id"], "paper:one");
    assert_eq!(manifest["papers"][0]["revision"], stored.sha256);
}

#[tokio::test]
async fn refuses_deleted_or_pathless_papers() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.insert_paper("paper:missing", "没有 revision")
        .await
        .unwrap();
    let builder = ConversationContextBuilder::new(db.clone(), workspace);
    assert!(builder
        .refresh("conversation-2", &[paper_scope("paper:missing")])
        .await
        .is_err());

    db.trash_paper("paper:missing").await.unwrap();
    assert!(builder
        .refresh("conversation-2", &[paper_scope("paper:missing")])
        .await
        .is_err());
}

#[tokio::test]
async fn project_scope_records_the_research_goal() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let project_id = db
        .create_project("ablation", "消融研究", "比较各个模块对最终结果的贡献")
        .await
        .unwrap();
    let scope = ConversationScope {
        conversation_id: "conversation-project".into(),
        scope_type: "project".into(),
        scope_id: Some(project_id),
        added_at: "2026-01-01 00:00:00".into(),
    };

    let bundle = ConversationContextBuilder::new(db, workspace)
        .refresh("conversation-project", &[scope])
        .await
        .unwrap();
    let summary = tokio::fs::read_to_string(bundle.summary_path)
        .await
        .unwrap();
    assert!(summary.contains("消融研究"));
    assert!(summary.contains("比较各个模块对最终结果的贡献"));
}

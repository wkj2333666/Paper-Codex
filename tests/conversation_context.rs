use paper_codex::{
    conversation_context::ConversationContextBuilder,
    conversations::{ConversationScope, ConversationScopeInput},
    db::Database,
    research::{EvidenceLevel, WorkMetadata},
    research_store::ResearchStore,
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

#[tokio::test]
async fn project_context_includes_bounded_project_notes_and_sibling_chat_memory() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let project_id = db
        .create_project("vision", "视觉编码器", "理解视觉表征")
        .await
        .unwrap();
    tokio::fs::create_dir_all(temp.path().join("projects/vision"))
        .await
        .unwrap();
    tokio::fs::write(
        temp.path().join("projects/vision/README.md"),
        "# 项目笔记\n\n重点比较 SigLIP 与 DINOv2 的 patch token。",
    )
    .await
    .unwrap();

    let earlier = db.create_conversation("DINO 入门").await.unwrap();
    db.replace_conversation_scopes(
        &earlier.id,
        &[ConversationScopeInput {
            scope_type: "project".into(),
            scope_id: Some(project_id.clone()),
        }],
    )
    .await
    .unwrap();
    db.append_chat_message(
        &earlier.id,
        "user",
        "EMA teacher 为什么不是预训练教师？",
        "completed",
    )
    .await
    .unwrap();
    db.append_chat_message(
        &earlier.id,
        "assistant",
        "teacher 是 student 参数的指数移动平均副本。",
        "completed",
    )
    .await
    .unwrap();

    let current = db.create_conversation("继续学习").await.unwrap();
    let scope = ConversationScope {
        conversation_id: current.id.clone(),
        scope_type: "project".into(),
        scope_id: Some(project_id),
        added_at: "2026-01-01 00:00:00".into(),
    };
    let bundle = ConversationContextBuilder::new(db, workspace)
        .refresh(&current.id, &[scope])
        .await
        .unwrap();
    let summary = tokio::fs::read_to_string(bundle.summary_path)
        .await
        .unwrap();

    assert!(summary.contains("## 当前项目笔记"));
    assert!(summary.contains("重点比较 SigLIP 与 DINOv2"));
    assert!(summary.contains("## 同项目近期对话记忆"));
    assert!(summary.contains("DINO 入门"));
    assert!(summary.contains("EMA teacher 为什么不是预训练教师"));
    assert!(summary.contains("只用于承接用户认知"));
}

#[tokio::test]
async fn project_chat_memory_never_crosses_project_boundaries_or_echoes_current_chat() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let project = db.create_project("inside", "当前项目", "").await.unwrap();
    let other = db.create_project("outside", "其他项目", "").await.unwrap();

    for (title, project_id, text) in [
        ("同项目旧对话", &project, "允许出现的记忆"),
        ("其他项目对话", &other, "绝不能出现的秘密"),
    ] {
        let conversation = db.create_conversation(title).await.unwrap();
        db.replace_conversation_scopes(
            &conversation.id,
            &[ConversationScopeInput {
                scope_type: "project".into(),
                scope_id: Some(project_id.clone()),
            }],
        )
        .await
        .unwrap();
        db.append_chat_message(&conversation.id, "user", text, "completed")
            .await
            .unwrap();
    }

    let current = db.create_conversation("当前对话").await.unwrap();
    db.replace_conversation_scopes(
        &current.id,
        &[ConversationScopeInput {
            scope_type: "project".into(),
            scope_id: Some(project.clone()),
        }],
    )
    .await
    .unwrap();
    db.append_chat_message(&current.id, "user", "当前消息不应被重复注入", "completed")
        .await
        .unwrap();
    let scope = ConversationScope {
        conversation_id: current.id.clone(),
        scope_type: "project".into(),
        scope_id: Some(project),
        added_at: String::new(),
    };
    let bundle = ConversationContextBuilder::new(db, workspace)
        .refresh(&current.id, &[scope])
        .await
        .unwrap();
    let summary = tokio::fs::read_to_string(bundle.summary_path)
        .await
        .unwrap();

    assert!(summary.contains("允许出现的记忆"));
    assert!(!summary.contains("绝不能出现的秘密"));
    assert!(!summary.contains("当前消息不应被重复注入"));
}

#[tokio::test]
async fn project_notes_and_chat_memory_are_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let project = db
        .create_project("bounded-memory", "有限上下文", "")
        .await
        .unwrap();
    tokio::fs::create_dir_all(temp.path().join("projects/bounded-memory"))
        .await
        .unwrap();
    tokio::fs::write(
        temp.path().join("projects/bounded-memory/README.md"),
        "笔".repeat(20_000),
    )
    .await
    .unwrap();
    let earlier = db.create_conversation("超长历史").await.unwrap();
    db.replace_conversation_scopes(
        &earlier.id,
        &[ConversationScopeInput {
            scope_type: "project".into(),
            scope_id: Some(project.clone()),
        }],
    )
    .await
    .unwrap();
    db.append_chat_message(&earlier.id, "assistant", &"忆".repeat(30_000), "completed")
        .await
        .unwrap();
    let current = db.create_conversation("当前").await.unwrap();
    let scope = ConversationScope {
        conversation_id: current.id.clone(),
        scope_type: "project".into(),
        scope_id: Some(project),
        added_at: String::new(),
    };

    let bundle = ConversationContextBuilder::new(db, workspace)
        .refresh(&current.id, &[scope])
        .await
        .unwrap();
    let summary = tokio::fs::read_to_string(bundle.summary_path)
        .await
        .unwrap();

    assert!(summary.chars().count() < 31_000);
    assert!(summary.contains(&format!("{}…", "笔".repeat(11_999))));
    assert!(summary.contains(&format!("{}…", "忆".repeat(1_999))));
}

#[tokio::test]
async fn project_context_contains_only_twenty_bounded_candidate_summaries() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let project_id = db
        .create_project("bounded", "候选摘要", "测试候选上下文")
        .await
        .unwrap();
    let store = ResearchStore::new(db.clone());
    for index in 0..25 {
        let work = store
            .upsert_work(WorkMetadata {
                canonical_key: format!("doi:10.1000/bounded-{index}"),
                doi: Some(format!("10.1000/bounded-{index}")),
                arxiv_id: None,
                openalex_id: None,
                title: format!("候选论文 {index:02}"),
                authors: vec![],
                year: Some(2025),
                abstract_text: Some(format!("不应进入上下文的摘要秘密 {index:02}")),
                source_url: format!("https://doi.org/10.1000/bounded-{index}"),
                pdf_url: None,
                evidence_level: EvidenceLevel::Abstract,
                metadata: serde_json::json!({}),
            })
            .await
            .unwrap();
        store
            .save_candidate(
                &project_id,
                &work.id,
                &format!("相关原因 {index:02}"),
                &[],
                None,
                None,
            )
            .await
            .unwrap();
    }
    let scope = ConversationScope {
        conversation_id: "conversation-bounded".into(),
        scope_type: "project".into(),
        scope_id: Some(project_id),
        added_at: "2026-01-01 00:00:00".into(),
    };
    let bundle = ConversationContextBuilder::new(db, workspace)
        .with_research_store(store)
        .refresh("conversation-bounded", &[scope])
        .await
        .unwrap();
    let summary = tokio::fs::read_to_string(bundle.summary_path)
        .await
        .unwrap();
    assert_eq!(summary.matches("- 候选：").count(), 20);
    assert!(!summary.contains("不应进入上下文的摘要秘密"));
    assert!(summary.contains("相关原因"));
    assert!(summary.contains("证据：abstract"));
}

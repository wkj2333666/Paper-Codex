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
async fn project_context_indexes_sibling_chats_without_injecting_their_messages() {
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

    for (paper_id, title, revision, evidence, takeaway) in [
        (
            "paper:dino",
            "DINO 论文",
            "revision-dino",
            "DINO evidence",
            "不应自动注入的 DINO 结论",
        ),
        (
            "paper:dp3",
            "DP3 论文",
            "revision-dp3",
            "DP3 evidence",
            "应直接提供的 DP3 结论",
        ),
    ] {
        db.insert_paper(paper_id, title).await.unwrap();
        db.add_paper_to_project(paper_id, &project_id)
            .await
            .unwrap();
        sqlx::query("UPDATE papers SET canonical_sha256=? WHERE id=?")
            .bind(revision)
            .bind(paper_id)
            .execute(db.pool())
            .await
            .unwrap();
        atomic_write(
            &workspace
                .state_dir()
                .join(format!("cache/extraction/{revision}/pages.md")),
            format!("<!-- page:1 -->\n{evidence}").as_bytes(),
        )
        .await
        .unwrap();
        db.upsert_paper_analysis(
            paper_id,
            revision,
            &serde_json::json!({"takeaway":takeaway}),
        )
        .await
        .unwrap();
    }

    let earlier = db.create_conversation("DINO 入门").await.unwrap();
    db.replace_conversation_scopes(
        &earlier.id,
        &[
            ConversationScopeInput {
                scope_type: "project".into(),
                scope_id: Some(project_id.clone()),
            },
            ConversationScopeInput {
                scope_type: "paper".into(),
                scope_id: Some("paper:dino".into()),
            },
        ],
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
    let scopes = [
        ConversationScope {
            conversation_id: current.id.clone(),
            scope_type: "project".into(),
            scope_id: Some(project_id),
            added_at: "2026-01-01 00:00:00".into(),
        },
        ConversationScope {
            conversation_id: current.id.clone(),
            scope_type: "paper".into(),
            scope_id: Some("paper:dp3".into()),
            added_at: "2026-01-01 00:00:00".into(),
        },
    ];
    let bundle = ConversationContextBuilder::new(db, workspace)
        .refresh(&current.id, &scopes)
        .await
        .unwrap();
    let summary = tokio::fs::read_to_string(bundle.summary_path)
        .await
        .unwrap();

    assert!(summary.contains("## 当前项目笔记"));
    assert!(summary.contains("重点比较 SigLIP 与 DINOv2"));
    assert!(summary.contains("## 同项目历史对话索引"));
    assert!(summary.contains("DINO 入门"));
    assert!(summary.contains("论文：`paper:dino` — DINO 论文"));
    assert!(summary.contains(&format!("history/{}.md", earlier.id)));
    assert!(!summary.contains("EMA teacher 为什么不是预训练教师"));
    assert!(!summary.contains("teacher 是 student 参数的指数移动平均副本"));
    assert!(summary.contains("应直接提供的 DP3 结论"));
    assert!(!summary.contains("不应自动注入的 DINO 结论"));
    assert!(summary.contains("`paper:dino` — DINO 论文"));
    assert!(summary.contains("papers/paper_dino-revision-dino.md"));

    let history = tokio::fs::read_to_string(bundle.root.join(format!("history/{}.md", earlier.id)))
        .await
        .unwrap();
    assert!(history.contains("EMA teacher 为什么不是预训练教师"));
    assert!(history.contains("teacher 是 student 参数的指数移动平均副本"));
    assert!(history.contains("`paper:dino` — DINO 论文"));
}

#[tokio::test]
async fn context_separates_global_profile_from_project_learning_state() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let project = db.create_project("memory", "教学记忆", "").await.unwrap();
    let other = db.create_project("other", "其他项目", "").await.unwrap();
    db.insert_memory_item(
        "global",
        None,
        "preference",
        "先给整体结构，再讲局部细节",
        "explicit_user",
        "high",
        None,
    )
    .await
    .unwrap();
    db.insert_memory_item(
        "project",
        Some(&project),
        "unresolved_concept",
        "稀疏三维卷积",
        "explicit_user",
        "high",
        None,
    )
    .await
    .unwrap();
    db.insert_memory_item(
        "project",
        Some(&other),
        "goal",
        "不属于当前项目的目标",
        "explicit_user",
        "high",
        None,
    )
    .await
    .unwrap();

    let conversation = db.create_conversation("记忆分层").await.unwrap();
    let scope = ConversationScope {
        conversation_id: conversation.id.clone(),
        scope_type: "project".into(),
        scope_id: Some(project),
        added_at: String::new(),
    };
    let bundle = ConversationContextBuilder::new(db, workspace)
        .refresh(&conversation.id, &[scope])
        .await
        .unwrap();
    let summary = tokio::fs::read_to_string(bundle.summary_path)
        .await
        .unwrap();

    assert!(summary.contains("## 用户画像"));
    assert!(summary.contains("先给整体结构，再讲局部细节"));
    assert!(summary.contains("## 当前项目学习状态"));
    assert!(summary.contains("稀疏三维卷积"));
    assert!(!summary.contains("不属于当前项目的目标"));
    assert!(summary.contains("## 论文与外部证据"));
    assert!(!summary.contains("不可信"));
}

#[tokio::test]
async fn project_chat_index_never_crosses_project_boundaries_or_echoes_current_chat() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let project = db.create_project("inside", "当前项目", "").await.unwrap();
    let other = db.create_project("outside", "其他项目", "").await.unwrap();

    let inside = db.create_conversation("同项目旧对话").await.unwrap();
    db.replace_conversation_scopes(
        &inside.id,
        &[ConversationScopeInput {
            scope_type: "project".into(),
            scope_id: Some(project.clone()),
        }],
    )
    .await
    .unwrap();
    db.append_chat_message(&inside.id, "user", "允许按需读取的记忆", "completed")
        .await
        .unwrap();
    let inside_id = inside.id;

    let outside = db.create_conversation("其他项目对话").await.unwrap();
    db.replace_conversation_scopes(
        &outside.id,
        &[ConversationScopeInput {
            scope_type: "project".into(),
            scope_id: Some(other),
        }],
    )
    .await
    .unwrap();
    db.append_chat_message(&outside.id, "user", "绝不能出现的秘密", "completed")
        .await
        .unwrap();
    let outside_id = outside.id;

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

    assert!(summary.contains("同项目旧对话"));
    assert!(!summary.contains("允许按需读取的记忆"));
    assert!(bundle
        .root
        .join(format!("history/{inside_id}.md"))
        .is_file());
    let history = tokio::fs::read_to_string(bundle.root.join(format!("history/{inside_id}.md")))
        .await
        .unwrap();
    assert!(history.contains("允许按需读取的记忆"));
    assert!(!summary.contains("绝不能出现的秘密"));
    assert!(!summary.contains("其他项目对话"));
    assert!(!bundle
        .root
        .join(format!("history/{outside_id}.md"))
        .exists());
    assert!(!summary.contains("当前消息不应被重复注入"));
    assert!(!bundle
        .root
        .join(format!("history/{}.md", current.id))
        .exists());
}

#[tokio::test]
async fn project_notes_and_on_demand_chat_files_are_bounded() {
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

    assert!(summary.chars().count() < 15_000);
    assert!(summary.contains(&format!("{}…", "笔".repeat(11_999))));
    assert!(!summary.contains('忆'));
    let history = tokio::fs::read_to_string(bundle.root.join(format!("history/{}.md", earlier.id)))
        .await
        .unwrap();
    assert!(history.contains(&format!("{}…", "忆".repeat(1_999))));
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

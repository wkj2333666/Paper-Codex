use paper_codex::{
    db::Database,
    domain::{Paper, PaperIdentity, TaskState},
    graph::{GraphEdge, GraphNode, KnowledgeKind},
};
use serde_json::json;

fn committed_paper(id: &str) -> Paper {
    Paper {
        id: id.into(),
        title: "Committed title".into(),
        authors_json: r#"["Committed Author"]"#.into(),
        year: Some(2024),
        doi: None,
        arxiv_id: Some("2402.05099".into()),
        canonical_sha256: Some("committed-revision".into()),
        source_url: Some("https://arxiv.org/abs/2402.05099".into()),
        note_path: Some("library/generated/papers/arxiv_2402.05099.md".into()),
        deleted_at: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    }
}

#[test]
fn normalizes_doi_and_arxiv_identities() {
    assert_eq!(
        PaperIdentity::parse("https://doi.org/10.1145/3290607.3299033")
            .unwrap()
            .to_string(),
        "doi:10.1145/3290607.3299033"
    );
    assert_eq!(
        PaperIdentity::parse("https://arxiv.org/pdf/1706.03762v7.pdf")
            .unwrap()
            .to_string(),
        "arxiv:1706.03762"
    );
    assert!(PaperIdentity::parse("some paper title").is_none());
}

#[test]
fn task_state_machine_rejects_skips_and_terminal_restarts() {
    assert!(TaskState::Queued.can_transition_to(TaskState::Resolving));
    assert!(!TaskState::Queued.can_transition_to(TaskState::Analyzing));
    assert!(TaskState::Analyzing.can_transition_to(TaskState::Failed));
    assert!(!TaskState::Done.can_transition_to(TaskState::Queued));
}

#[tokio::test]
async fn a_paper_can_belong_to_multiple_projects_without_duplication() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.insert_paper("doi:10.1/example", "Example Paper")
        .await
        .unwrap();
    let first = db
        .create_project("first", "First", "First angle")
        .await
        .unwrap();
    let second = db
        .create_project("second", "Second", "Second angle")
        .await
        .unwrap();

    db.add_paper_to_project("doi:10.1/example", &first)
        .await
        .unwrap();
    db.add_paper_to_project("doi:10.1/example", &second)
        .await
        .unwrap();
    db.add_paper_to_project("doi:10.1/example", &first)
        .await
        .unwrap();

    let projects = db.paper_project_ids("doi:10.1/example").await.unwrap();
    assert_eq!(projects.len(), 2);
    assert!(projects.contains(&first));
    assert!(projects.contains(&second));
    assert_eq!(db.paper_count().await.unwrap(), 1);
}

#[tokio::test]
async fn provisional_duplicate_registration_preserves_committed_metadata_and_memberships() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let committed = committed_paper("arxiv:2402.05099");
    db.upsert_paper(&committed).await.unwrap();
    let first = db.create_project("first", "First", "").await.unwrap();
    let second = db.create_project("second", "Second", "").await.unwrap();
    db.add_paper_to_project(&committed.id, &first)
        .await
        .unwrap();
    db.add_paper_to_project(&committed.id, &second)
        .await
        .unwrap();
    let mut provisional = committed.clone();
    provisional.title = "https://arxiv.org/abs/2402.05099".into();
    provisional.authors_json = "[]".into();
    provisional.year = None;
    provisional.canonical_sha256 = Some("new-unvalidated-revision".into());
    provisional.note_path = None;

    db.register_provisional_paper(&provisional).await.unwrap();

    let stored = db.get_paper(&committed.id).await.unwrap().unwrap();
    assert_eq!(stored.title, committed.title);
    assert_eq!(stored.authors_json, committed.authors_json);
    assert_eq!(stored.year, committed.year);
    assert_eq!(stored.canonical_sha256, committed.canonical_sha256);
    assert_eq!(stored.note_path, committed.note_path);
    assert_eq!(
        db.paper_project_ids(&committed.id).await.unwrap().len(),
        2
    );
}

#[tokio::test]
async fn paper_analysis_records_the_completing_model_and_effort() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.insert_paper("paper:one", "Paper").await.unwrap();

    db.upsert_paper_analysis_with_provenance(
        "paper:one",
        "rev-1",
        &json!({"takeaway":"result"}),
        "gpt-5.6-terra",
        "medium",
    )
    .await
    .unwrap();

    assert_eq!(
        db.paper_analysis_provenance("paper:one").await.unwrap(),
        Some(("gpt-5.6-terra".into(), "medium".into()))
    );
}

#[tokio::test]
async fn schema_migrates_projects_papers_analyses_and_graph_idempotently() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.insert_paper("paper:one", "第一篇论文").await.unwrap();

    let root = db
        .create_project_with_parent("root", "根项目", "", None)
        .await
        .unwrap();
    let child = db
        .create_project_with_parent("child", "子项目", "", Some(&root))
        .await
        .unwrap();
    assert_eq!(
        db.get_project(&child).await.unwrap().unwrap().parent_id,
        Some(root)
    );

    let analysis = json!({
        "takeaway": "这是一句话结论。",
        "research_question": "论文解决什么问题？",
        "method": "一种方法",
        "results": ["结果一"],
        "limitations": ["限制一"]
    });
    db.upsert_paper_analysis("paper:one", "rev-1", &analysis)
        .await
        .unwrap();
    assert_eq!(
        db.paper_analysis("paper:one").await.unwrap().unwrap()["takeaway"],
        "这是一句话结论。"
    );

    let nodes = vec![
        GraphNode::paper("paper:one", "第一篇论文"),
        GraphNode::knowledge("method:attention", KnowledgeKind::Method, "注意力机制"),
    ];
    let edges = vec![GraphEdge::formal(
        "paper:one:uses-method:method:attention",
        "paper:one",
        "method:attention",
        "uses-method",
        json!([{"page": 3, "kind": "statement"}]),
    )];
    db.replace_paper_graph("paper:one", "rev-1", &nodes, &edges)
        .await
        .unwrap();
    let graph = db.graph_for_paper("paper:one").await.unwrap();
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);
    assert!(!graph.edges[0].hypothesis);

    db.trash_paper("paper:one").await.unwrap();
    assert!(db.list_papers().await.unwrap().is_empty());
    assert_eq!(db.list_trashed_papers().await.unwrap().len(), 1);
    db.restore_paper("paper:one").await.unwrap();
    assert_eq!(db.list_papers().await.unwrap().len(), 1);
}

#[tokio::test]
async fn nested_project_lifecycle_never_deletes_library_papers() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.insert_paper("paper:one", "第一篇论文").await.unwrap();
    db.insert_paper("paper:two", "第二篇论文").await.unwrap();
    let root = db
        .create_project_with_parent("root", "根项目", "", None)
        .await
        .unwrap();
    let child = db
        .create_project_with_parent("child", "子项目", "", Some(&root))
        .await
        .unwrap();
    let grandchild = db
        .create_project_with_parent("grandchild", "孙项目", "", Some(&child))
        .await
        .unwrap();
    db.add_paper_to_project("paper:one", &child).await.unwrap();
    db.add_paper_to_project("paper:two", &grandchild)
        .await
        .unwrap();

    assert!(db
        .update_project(&root, "根项目", "", Some(&grandchild))
        .await
        .is_err());

    let impact = db.project_impact(&child).await.unwrap();
    assert_eq!(impact.direct_papers, 1);
    assert_eq!(impact.descendant_projects, 1);
    assert_eq!(impact.descendant_papers, 1);

    db.delete_project(&child, false).await.unwrap();
    assert!(db.get_project(&child).await.unwrap().is_none());
    assert_eq!(
        db.get_project(&grandchild)
            .await
            .unwrap()
            .unwrap()
            .parent_id,
        Some(root.clone())
    );
    assert_eq!(db.paper_count().await.unwrap(), 2);
    assert!(db.paper_project_ids("paper:one").await.unwrap().is_empty());

    db.delete_project(&root, true).await.unwrap();
    assert!(db.list_projects().await.unwrap().is_empty());
    assert_eq!(db.paper_count().await.unwrap(), 2);
    assert!(db.paper_project_ids("paper:two").await.unwrap().is_empty());
}

#[tokio::test]
async fn permanent_paper_deletion_requires_trash_and_reports_impact() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.insert_paper("paper:one", "第一篇论文").await.unwrap();
    let project = db.create_project("project", "项目", "").await.unwrap();
    db.add_paper_to_project("paper:one", &project)
        .await
        .unwrap();

    let impact = db.paper_impact("paper:one").await.unwrap();
    assert_eq!(impact.project_references, 1);
    assert!(db.permanently_delete_paper("paper:one").await.is_err());

    db.trash_paper("paper:one").await.unwrap();
    db.permanently_delete_paper("paper:one").await.unwrap();
    assert!(db.get_paper("paper:one").await.unwrap().is_none());
}

use paper_codex::{
    db::Database,
    graph::{materialize_proposal, GraphEdge, GraphNode, KnowledgeKind},
    knowledge::{Evidence, KnowledgeEntity, PaperNote, ProposedKnowledge, SemanticRelation},
};
use serde_json::json;

fn evidence(page: u32) -> Evidence {
    Evidence {
        paper_id: "paper:one".into(),
        revision: "rev-1".into(),
        page,
        section: Some("2".into()),
        locator: None,
        kind: "statement".into(),
    }
}

fn proposal() -> ProposedKnowledge {
    ProposedKnowledge {
        paper: PaperNote {
            paper_id: "paper:one".into(),
            revision: "rev-1".into(),
            title: "第一篇论文".into(),
            authors: vec![],
            year: Some(2024),
            takeaway: "一句话".into(),
            research_question: "问题".into(),
            contribution: "贡献".into(),
            method: "方法".into(),
            experimental_design: "实验".into(),
            baselines: vec![],
            results: vec!["结果".into()],
            limitations: vec!["局限".into()],
            assumptions: vec![],
            reproducibility: "部分".into(),
            evidence: vec![evidence(1)],
        },
        relations: vec![],
        entities: vec![
            KnowledgeEntity {
                key: "attention-a".into(),
                kind: "method".into(),
                name: "多头注意力".into(),
                description: "并行建模多个表示子空间".into(),
                evidence: vec![evidence(2)],
            },
            KnowledgeEntity {
                key: "finding-a".into(),
                kind: "finding".into(),
                name: "训练可并行化".into(),
                description: "避免循环结构的顺序依赖".into(),
                evidence: vec![evidence(3)],
            },
        ],
        semantic_relations: vec![
            SemanticRelation {
                source_key: "paper".into(),
                target_key: "attention-a".into(),
                relation_type: "uses-method".into(),
                hypothesis: false,
                confidence: 0.98,
                evidence: vec![evidence(2)],
            },
            SemanticRelation {
                source_key: "attention-a".into(),
                target_key: "finding-a".into(),
                relation_type: "supports".into(),
                hypothesis: true,
                confidence: 0.62,
                evidence: vec![],
            },
        ],
        recommended_projects: vec![],
    }
}

#[test]
fn materialization_uses_semantic_ids_and_preserves_evidence_status() {
    let graph = materialize_proposal(&proposal());
    assert!(graph
        .nodes
        .iter()
        .any(|node| { node.id == "method:多头注意力" && node.kind == KnowledgeKind::Method }));
    let formal = graph
        .edges
        .iter()
        .find(|edge| edge.relation_type == "uses-method")
        .unwrap();
    assert!(!formal.hypothesis);
    assert_eq!(formal.evidence[0]["page"], 2);
    let hypothesis = graph
        .edges
        .iter()
        .find(|edge| edge.relation_type == "supports")
        .unwrap();
    assert!(hypothesis.hypothesis);
    assert_eq!(hypothesis.confidence, 0.62);
}

#[tokio::test]
async fn graph_query_scopes_projects_filters_hypotheses_and_hides_trash() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.insert_paper("paper:one", "第一篇论文").await.unwrap();
    db.insert_paper("paper:two", "第二篇论文").await.unwrap();
    let project = db.create_project("p", "项目", "").await.unwrap();
    db.add_paper_to_project("paper:one", &project)
        .await
        .unwrap();

    let one_nodes = vec![
        GraphNode::paper("paper:one", "第一篇论文"),
        GraphNode::knowledge("method:one", KnowledgeKind::Method, "方法一"),
    ];
    let one_edges = vec![GraphEdge::formal(
        "edge:one",
        "paper:one",
        "method:one",
        "uses-method",
        json!([{"page": 1}]),
    )];
    db.replace_paper_graph("paper:one", "rev-1", &one_nodes, &one_edges)
        .await
        .unwrap();
    let two_nodes = vec![
        GraphNode::paper("paper:two", "第二篇论文"),
        GraphNode::knowledge("concept:two", KnowledgeKind::Concept, "概念二"),
    ];
    let mut hypothesis = GraphEdge::formal(
        "edge:two",
        "paper:two",
        "concept:two",
        "introduces",
        json!([]),
    );
    hypothesis.hypothesis = true;
    hypothesis.confidence = 0.55;
    db.replace_paper_graph("paper:two", "rev-1", &two_nodes, &[hypothesis])
        .await
        .unwrap();

    let project_graph = db
        .query_graph(Some(&project), None, &[], true)
        .await
        .unwrap();
    assert!(project_graph
        .nodes
        .iter()
        .any(|node| node.id == "paper:one"));
    assert!(!project_graph
        .nodes
        .iter()
        .any(|node| node.id == "paper:two"));

    let formal_only = db.query_graph(None, None, &[], false).await.unwrap();
    assert!(formal_only.edges.iter().all(|edge| !edge.hypothesis));

    db.trash_paper("paper:one").await.unwrap();
    let global = db.query_graph(None, None, &[], true).await.unwrap();
    assert!(!global.nodes.iter().any(|node| node.id == "paper:one"));
    assert!(global.nodes.iter().any(|node| node.id == "paper:two"));
}

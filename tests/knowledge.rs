use paper_codex::{
    knowledge::{
        analysis_from_markdown, proposal_schema, Evidence, KnowledgeEntity, KnowledgeRepository,
        PaperNote, ProposedKnowledge, Relation, SemanticRelation,
    },
    prompts::first_pass_prompt,
    workspace::Workspace,
};
use serde_json::Value;

fn proposal(page: u32) -> ProposedKnowledge {
    ProposedKnowledge {
        paper: PaperNote {
            paper_id: "doi:10.1/example".into(),
            revision: "abc123".into(),
            title: "Example".into(),
            authors: vec!["A. Author".into()],
            year: Some(2025),
            takeaway: "一句话结论".into(),
            research_question: "Question".into(),
            contribution: "Contribution".into(),
            method: "Method".into(),
            experimental_design: "Design".into(),
            baselines: vec!["Baseline".into()],
            results: vec!["Result".into()],
            limitations: vec!["Limit".into()],
            assumptions: vec![],
            reproducibility: "Partial".into(),
            evidence: vec![Evidence {
                paper_id: "doi:10.1/example".into(),
                revision: "abc123".into(),
                page,
                section: Some("1".into()),
                locator: None,
                kind: "statement".into(),
            }],
        },
        relations: vec![Relation {
            target_paper_id: "doi:10.2/related".into(),
            relation_type: "extends".into(),
            hypothesis: true,
            evidence: vec![],
        }],
        entities: vec![KnowledgeEntity {
            key: "attention".into(),
            kind: "method".into(),
            name: "注意力机制".into(),
            description: "直接建模序列位置之间的依赖。".into(),
            evidence: vec![Evidence {
                paper_id: "doi:10.1/example".into(),
                revision: "abc123".into(),
                page,
                section: Some("2".into()),
                locator: None,
                kind: "definition".into(),
            }],
        }],
        semantic_relations: vec![SemanticRelation {
            source_key: "paper".into(),
            target_key: "attention".into(),
            relation_type: "uses-method".into(),
            hypothesis: false,
            confidence: 0.96,
            evidence: vec![Evidence {
                paper_id: "doi:10.1/example".into(),
                revision: "abc123".into(),
                page,
                section: Some("2".into()),
                locator: None,
                kind: "statement".into(),
            }],
        }],
        recommended_projects: vec![],
    }
}

#[test]
fn app_server_schema_is_strict_for_every_nested_object() {
    fn assert_strict(value: &Value) {
        if value.get("type").and_then(Value::as_str) == Some("object") {
            assert_eq!(value.get("additionalProperties"), Some(&Value::Bool(false)));
            if let Some(properties) = value.get("properties").and_then(Value::as_object) {
                let required = value
                    .get("required")
                    .and_then(Value::as_array)
                    .expect("object properties must all be required");
                for key in properties.keys() {
                    assert!(required.iter().any(|item| item.as_str() == Some(key)));
                }
            }
        }
        match value {
            Value::Array(items) => items.iter().for_each(assert_strict),
            Value::Object(items) => items.values().for_each(assert_strict),
            _ => {}
        }
    }

    assert_strict(&proposal_schema());
}

#[test]
fn app_server_schema_exposes_validator_vocabularies() {
    let schema = proposal_schema();
    let evidence_kinds = schema
        .pointer("/$defs/EvidenceKindSchema/enum")
        .and_then(Value::as_array)
        .expect("evidence kind must be an enum");
    assert_eq!(
        schema.pointer("/$defs/Evidence/properties/kind/$ref"),
        Some(&Value::String("#/$defs/EvidenceKindSchema".into()))
    );
    assert!(evidence_kinds.iter().any(|value| value == "statement"));
    assert!(!evidence_kinds.iter().any(|value| value == "bibliographic"));

    let relation_types = schema
        .pointer("/$defs/RelationTypeSchema/enum")
        .and_then(Value::as_array)
        .expect("relation type must be an enum");
    assert_eq!(
        schema.pointer("/$defs/Relation/properties/relation_type/$ref"),
        Some(&Value::String("#/$defs/RelationTypeSchema".into()))
    );
    assert!(relation_types.iter().any(|value| value == "reuses-method"));
    assert!(relation_types.iter().any(|value| value == "uses-dataset"));
}

#[tokio::test]
async fn rejects_invalid_evidence_and_protected_commit_paths() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let repository = KnowledgeRepository::new(workspace);
    assert!(repository.validate(&proposal(0)).is_err());
    let mut mismatched = proposal(2);
    mismatched.paper.evidence[0].paper_id = "doi:10.9/wrong".into();
    assert!(repository.validate(&mismatched).is_err());
    assert!(!temp.path().join("library/raw/papers/changed").exists());
    assert!(!temp.path().join("annotations/papers/changed").exists());
}

#[tokio::test]
async fn commits_valid_note_and_detects_base_hash_conflicts() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(temp.path()).await.unwrap();
    let repository = KnowledgeRepository::new(workspace);
    let first = repository
        .commit("task-1", &proposal(2), None)
        .await
        .unwrap();
    let original = tokio::fs::read_to_string(&first.note_path).await.unwrap();
    assert!(original.contains("## 一句话结论"));
    assert!(original.contains("## 研究问题"));
    assert!(!original.contains("## Evidence"));
    assert!(!original.contains("relations:"));
    assert!(repository
        .commit("task-2", &proposal(3), Some("wrong-base-hash"))
        .await
        .is_err());
    assert_eq!(
        tokio::fs::read_to_string(&first.note_path).await.unwrap(),
        original
    );
}

#[test]
fn legacy_markdown_becomes_a_structured_chinese_reading_view() {
    let legacy = r#"---
type: paper
paper_id: arxiv:1706.03762
evidence:
  - page: 1
---
# Attention Is All You Need

## Research question

Can sequence transduction avoid recurrence?

## Contribution

The Transformer architecture.

## Method

Multi-head self-attention.

## Results

- 28.4 BLEU on WMT 2014 English-to-German.

## Limitations

- Evaluation focuses on machine translation.
"#;
    let analysis = analysis_from_markdown(legacy).expect("legacy analysis");
    assert_eq!(
        analysis["research_question"],
        "Can sequence transduction avoid recurrence?"
    );
    assert_eq!(analysis["method"], "Multi-head self-attention.");
    assert_eq!(
        analysis["results"][0],
        "28.4 BLEU on WMT 2014 English-to-German."
    );
    assert!(analysis.get("evidence").is_none());
}

#[test]
fn codex_prompt_requires_concise_chinese_and_evidence_graded_graph_output() {
    let prompt = first_pass_prompt(std::path::Path::new("paper.md"), "paper:one", "rev-1", "[]");
    assert!(prompt.contains("使用简体中文"));
    assert!(prompt.contains("一句话结论"));
    assert!(prompt.contains("最多 3 条"));
    assert!(prompt.contains("不得嵌入作者/分析者标签或证据编号"));
    assert!(prompt.contains("假设关系"));
}

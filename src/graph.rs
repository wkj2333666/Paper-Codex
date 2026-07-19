use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::knowledge::ProposedKnowledge;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum KnowledgeKind {
    Paper,
    Concept,
    Method,
    Dataset,
    Finding,
}

impl KnowledgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paper => "paper",
            Self::Concept => "concept",
            Self::Method => "method",
            Self::Dataset => "dataset",
            Self::Finding => "finding",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: KnowledgeKind,
    pub label: String,
    pub description: String,
    pub paper_id: Option<String>,
}

impl GraphNode {
    pub fn paper(id: impl Into<String>, label: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            id: id.clone(),
            kind: KnowledgeKind::Paper,
            label: label.into(),
            description: String::new(),
            paper_id: Some(id),
        }
    }

    pub fn knowledge(id: impl Into<String>, kind: KnowledgeKind, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            label: label.into(),
            description: String::new(),
            paper_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub relation_type: String,
    pub hypothesis: bool,
    pub confidence: f64,
    pub evidence: Value,
}

impl GraphEdge {
    pub fn formal(
        id: impl Into<String>,
        source: impl Into<String>,
        target: impl Into<String>,
        relation_type: impl Into<String>,
        evidence: Value,
    ) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            target: target.into(),
            relation_type: relation_type.into(),
            hypothesis: false,
            confidence: 1.0,
            evidence,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphPayload {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

pub fn materialize_proposal(proposal: &ProposedKnowledge) -> GraphPayload {
    let paper = &proposal.paper;
    let mut nodes = vec![GraphNode::paper(&paper.paper_id, &paper.title)];
    let mut keys = HashMap::from([("paper".to_owned(), paper.paper_id.clone())]);
    for entity in &proposal.entities {
        let kind = match entity.kind.as_str() {
            "concept" => KnowledgeKind::Concept,
            "method" => KnowledgeKind::Method,
            "dataset" => KnowledgeKind::Dataset,
            "finding" => KnowledgeKind::Finding,
            _ => continue,
        };
        let id = format!("{}:{}", kind.as_str(), semantic_key(&entity.name));
        keys.insert(entity.key.clone(), id.clone());
        nodes.push(GraphNode {
            id,
            kind,
            label: entity.name.trim().to_owned(),
            description: entity.description.trim().to_owned(),
            paper_id: None,
        });
    }
    let mut edges = Vec::new();
    for (index, relation) in proposal.semantic_relations.iter().enumerate() {
        let (Some(source), Some(target)) = (
            keys.get(&relation.source_key),
            keys.get(&relation.target_key),
        ) else {
            continue;
        };
        edges.push(GraphEdge {
            id: format!(
                "semantic:{}:{}:{}:{}",
                paper.paper_id, relation.relation_type, source, index
            ),
            source: source.clone(),
            target: target.clone(),
            relation_type: relation.relation_type.clone(),
            hypothesis: relation.hypothesis,
            confidence: relation.confidence,
            evidence: serde_json::to_value(&relation.evidence)
                .unwrap_or_else(|_| Value::Array(vec![])),
        });
    }
    GraphPayload { nodes, edges }
}

pub fn materialize_legacy_analysis(paper_id: &str, title: &str, analysis: &Value) -> GraphPayload {
    let mut nodes = vec![GraphNode::paper(paper_id, title)];
    let mut edges = Vec::new();
    if let Some(method) = analysis.get("method").and_then(Value::as_str) {
        if !method.trim().is_empty() {
            let label = concise_label(method, 48);
            let id = format!("method:{}", semantic_key(&label));
            nodes.push(GraphNode {
                id: id.clone(),
                kind: KnowledgeKind::Method,
                label,
                description: method.trim().to_owned(),
                paper_id: None,
            });
            edges.push(legacy_edge(paper_id, &id, "uses-method", 0));
        }
    }
    if let Some(results) = analysis.get("results").and_then(Value::as_array) {
        for (index, result) in results.iter().filter_map(Value::as_str).take(3).enumerate() {
            let label = concise_label(result, 48);
            let id = format!("finding:{}", semantic_key(&label));
            nodes.push(GraphNode {
                id: id.clone(),
                kind: KnowledgeKind::Finding,
                label,
                description: result.trim().to_owned(),
                paper_id: None,
            });
            edges.push(legacy_edge(paper_id, &id, "reports", index + 1));
        }
    }
    GraphPayload { nodes, edges }
}

fn legacy_edge(paper_id: &str, target: &str, relation_type: &str, index: usize) -> GraphEdge {
    GraphEdge {
        id: format!("legacy:{paper_id}:{relation_type}:{index}"),
        source: paper_id.to_owned(),
        target: target.to_owned(),
        relation_type: relation_type.to_owned(),
        hypothesis: true,
        confidence: 0.5,
        evidence: Value::Array(vec![]),
    }
}

fn concise_label(value: &str, max_chars: usize) -> String {
    let first = value.lines().next().unwrap_or(value).trim();
    let mut label = first.chars().take(max_chars).collect::<String>();
    if first.chars().count() > max_chars {
        label.push('…');
    }
    label
}

fn semantic_key(value: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in value.trim().to_lowercase().chars() {
        if character.is_alphanumeric() {
            output.push(character);
            separator = false;
        } else if !separator && !output.is_empty() {
            output.push('-');
            separator = true;
        }
    }
    output.trim_end_matches('-').to_owned()
}

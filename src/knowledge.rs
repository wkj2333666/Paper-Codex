use crate::workspace::{atomic_write, safe_key, Workspace};
use anyhow::{bail, Context, Result};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(JsonSchema)]
#[schemars(rename_all = "kebab-case")]
#[allow(dead_code)]
enum EvidenceKindSchema {
    Statement,
    Definition,
    Derivation,
    Experiment,
    Figure,
    Table,
    Appendix,
}

#[derive(JsonSchema)]
#[schemars(rename_all = "kebab-case")]
#[allow(dead_code)]
enum RelationTypeSchema {
    Cites,
    Supports,
    Contradicts,
    Extends,
    ReusesMethod,
    UsesDataset,
    ComparesWith,
    Replicates,
    Supersedes,
}

#[derive(JsonSchema)]
#[schemars(rename_all = "kebab-case")]
#[allow(dead_code)]
enum KnowledgeKindSchema {
    Concept,
    Method,
    Dataset,
    Finding,
}

#[derive(JsonSchema)]
#[schemars(rename_all = "kebab-case")]
#[allow(dead_code)]
enum SemanticRelationTypeSchema {
    Introduces,
    Defines,
    UsesMethod,
    UsesDataset,
    Reports,
    Supports,
    Contradicts,
    Extends,
    Evaluates,
    RelatedTo,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Evidence {
    pub paper_id: String,
    pub revision: String,
    pub page: u32,
    pub section: Option<String>,
    pub locator: Option<String>,
    #[schemars(with = "EvidenceKindSchema")]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PaperNote {
    pub paper_id: String,
    pub revision: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i64>,
    pub takeaway: String,
    pub research_question: String,
    pub contribution: String,
    pub method: String,
    pub experimental_design: String,
    pub baselines: Vec<String>,
    pub results: Vec<String>,
    pub limitations: Vec<String>,
    pub assumptions: Vec<String>,
    pub reproducibility: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KnowledgeEntity {
    pub key: String,
    #[schemars(with = "KnowledgeKindSchema")]
    pub kind: String,
    pub name: String,
    pub description: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticRelation {
    pub source_key: String,
    pub target_key: String,
    #[schemars(with = "SemanticRelationTypeSchema")]
    pub relation_type: String,
    pub hypothesis: bool,
    pub confidence: f64,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Relation {
    pub target_paper_id: String,
    #[schemars(with = "RelationTypeSchema")]
    pub relation_type: String,
    pub hypothesis: bool,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProposedKnowledge {
    pub paper: PaperNote,
    pub relations: Vec<Relation>,
    pub entities: Vec<KnowledgeEntity>,
    pub semantic_relations: Vec<SemanticRelation>,
    pub recommended_projects: Vec<String>,
}

pub fn proposal_schema() -> Value {
    let mut schema =
        serde_json::to_value(schema_for!(ProposedKnowledge)).unwrap_or(json!({"type":"object"}));
    strictify_schema(&mut schema);
    schema
}

fn strictify_schema(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("object") {
                object.insert("additionalProperties".into(), Value::Bool(false));
                if let Some(properties) = object.get("properties").and_then(Value::as_object) {
                    let required = properties.keys().cloned().map(Value::String).collect();
                    object.insert("required".into(), Value::Array(required));
                }
            }
            object.values_mut().for_each(strictify_schema);
        }
        Value::Array(items) => items.iter_mut().for_each(strictify_schema),
        _ => {}
    }
}

#[derive(Debug, Clone)]
pub struct CommitResult {
    pub note_path: PathBuf,
    pub content_hash: String,
}

#[derive(Clone)]
pub struct KnowledgeRepository {
    workspace: Workspace,
}

impl KnowledgeRepository {
    pub fn new(workspace: Workspace) -> Self {
        Self { workspace }
    }

    pub fn validate(&self, proposal: &ProposedKnowledge) -> Result<()> {
        if proposal.paper.paper_id.trim().is_empty() || proposal.paper.revision.trim().is_empty() {
            bail!("paper id and revision are required");
        }
        if proposal.paper.evidence.is_empty() {
            bail!("paper note requires evidence");
        }
        if proposal.paper.takeaway.trim().is_empty() {
            bail!("paper takeaway is required");
        }
        for evidence in &proposal.paper.evidence {
            validate_evidence(evidence)?;
            if evidence.paper_id != proposal.paper.paper_id
                || evidence.revision != proposal.paper.revision
            {
                bail!("paper evidence must reference the current paper revision");
            }
        }
        let relation_types = [
            "cites",
            "supports",
            "contradicts",
            "extends",
            "reuses-method",
            "uses-dataset",
            "compares-with",
            "replicates",
            "supersedes",
        ];
        for relation in &proposal.relations {
            if !relation_types.contains(&relation.relation_type.as_str()) {
                bail!("invalid relation type: {}", relation.relation_type);
            }
            if !relation.hypothesis && relation.evidence.is_empty() {
                bail!("formal relation requires evidence");
            }
            for evidence in &relation.evidence {
                validate_evidence(evidence)?;
            }
        }
        let entity_kinds = ["concept", "method", "dataset", "finding"];
        let mut entity_keys = std::collections::HashSet::from(["paper"]);
        for entity in &proposal.entities {
            if entity.key.trim().is_empty()
                || entity.name.trim().is_empty()
                || !entity_kinds.contains(&entity.kind.as_str())
            {
                bail!("invalid knowledge entity: {}", entity.key);
            }
            if !entity_keys.insert(entity.key.as_str()) {
                bail!("duplicate knowledge entity key: {}", entity.key);
            }
            if entity.evidence.is_empty() {
                bail!("knowledge entity requires evidence: {}", entity.key);
            }
            for evidence in &entity.evidence {
                validate_current_evidence(evidence, &proposal.paper)?;
            }
        }
        let semantic_types = [
            "introduces",
            "defines",
            "uses-method",
            "uses-dataset",
            "reports",
            "supports",
            "contradicts",
            "extends",
            "evaluates",
            "related-to",
        ];
        for relation in &proposal.semantic_relations {
            if !entity_keys.contains(relation.source_key.as_str())
                || !entity_keys.contains(relation.target_key.as_str())
                || !semantic_types.contains(&relation.relation_type.as_str())
                || !(0.0..=1.0).contains(&relation.confidence)
            {
                bail!("invalid semantic relation");
            }
            if !relation.hypothesis && relation.evidence.is_empty() {
                bail!("formal semantic relation requires evidence");
            }
            for evidence in &relation.evidence {
                validate_current_evidence(evidence, &proposal.paper)?;
            }
        }
        Ok(())
    }

    pub async fn commit(
        &self,
        task_id: &str,
        proposal: &ProposedKnowledge,
        base_hash: Option<&str>,
    ) -> Result<CommitResult> {
        self.validate(proposal)?;
        let relative = format!(
            "library/generated/papers/{}.md",
            safe_key(&proposal.paper.paper_id)
        );
        let target = self.workspace.generated_target(&relative)?;
        let current = tokio::fs::read(&target).await.ok();
        let current_hash = current.as_deref().map(hash_bytes);
        if current.is_some() && base_hash != current_hash.as_deref() {
            bail!("base content hash conflict");
        }
        let markdown = render_markdown(proposal)?;
        let stage = self.workspace.staging_dir(task_id);
        tokio::fs::create_dir_all(&stage).await?;
        let staged_note = stage.join("paper.md");
        atomic_write(
            &stage.join("proposal.json"),
            &serde_json::to_vec_pretty(proposal)?,
        )
        .await?;
        atomic_write(&staged_note, markdown.as_bytes()).await?;
        let journal_path = self
            .workspace
            .state_dir()
            .join("commit-journal")
            .join(format!("{task_id}.json"));
        let mut journal = json!({"task_id":task_id,"target":relative,"staged":staged_note,"base_hash":base_hash,"applied":false});
        atomic_write(&journal_path, &serde_json::to_vec_pretty(&journal)?).await?;
        atomic_write(&target, markdown.as_bytes()).await?;
        journal["applied"] = Value::Bool(true);
        atomic_write(&journal_path, &serde_json::to_vec_pretty(&journal)?).await?;
        Ok(CommitResult {
            note_path: target,
            content_hash: hash_bytes(markdown.as_bytes()),
        })
    }

    pub async fn recover(&self) -> Result<usize> {
        let dir = self.workspace.state_dir().join("commit-journal");
        let mut entries = tokio::fs::read_dir(&dir).await?;
        let mut recovered = 0;
        while let Some(entry) = entries.next_entry().await? {
            let bytes = tokio::fs::read(entry.path()).await?;
            let mut journal: Value = serde_json::from_slice(&bytes)?;
            if journal.get("applied").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            let relative = journal
                .get("target")
                .and_then(Value::as_str)
                .context("journal target missing")?;
            let staged = journal
                .get("staged")
                .and_then(Value::as_str)
                .context("journal staged path missing")?;
            let target = self.workspace.generated_target(relative)?;
            let content = tokio::fs::read(staged).await?;
            atomic_write(&target, &content).await?;
            journal["applied"] = Value::Bool(true);
            atomic_write(&entry.path(), &serde_json::to_vec_pretty(&journal)?).await?;
            recovered += 1;
        }
        Ok(recovered)
    }
}

fn validate_evidence(evidence: &Evidence) -> Result<()> {
    let kinds = [
        "statement",
        "definition",
        "derivation",
        "experiment",
        "figure",
        "table",
        "appendix",
    ];
    if evidence.paper_id.is_empty() || evidence.revision.is_empty() || evidence.page == 0 {
        bail!("invalid evidence locator");
    }
    if !kinds.contains(&evidence.kind.as_str()) {
        bail!("invalid evidence kind: {}", evidence.kind);
    }
    Ok(())
}

fn validate_current_evidence(evidence: &Evidence, paper: &PaperNote) -> Result<()> {
    validate_evidence(evidence)?;
    if evidence.paper_id != paper.paper_id || evidence.revision != paper.revision {
        bail!("evidence must reference the current paper revision");
    }
    Ok(())
}

fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn render_markdown(proposal: &ProposedKnowledge) -> Result<String> {
    let paper = &proposal.paper;
    Ok(format!(
        r#"---
type: paper
paper_id: {paper_id}
revision: {revision}
title: {title:?}
year: {year}
authors: {authors}
---

# {title}

## 一句话结论

{takeaway}

## 研究问题

{research_question}

## 核心贡献

{contribution}

## 方法

{method}

## 实验设计

{experimental_design}

## 对比基线

{baselines}

## 关键结果

{results}

## 局限

{limitations}

## 前提与假设

{assumptions}

## 可复现性

{reproducibility}
"#,
        paper_id = paper.paper_id,
        revision = paper.revision,
        title = paper.title,
        year = paper
            .year
            .map(|y| y.to_string())
            .unwrap_or_else(|| "null".into()),
        authors = serde_json::to_string(&paper.authors)?,
        takeaway = paper.takeaway,
        research_question = paper.research_question,
        contribution = paper.contribution,
        method = paper.method,
        experimental_design = paper.experimental_design,
        baselines = bullets(&paper.baselines),
        results = bullets(&paper.results),
        limitations = bullets(&paper.limitations),
        assumptions = bullets(&paper.assumptions),
        reproducibility = paper.reproducibility
    ))
}

fn bullets(values: &[String]) -> String {
    if values.is_empty() {
        "- 暂无可靠信息".into()
    } else {
        values
            .iter()
            .map(|v| format!("- {v}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn analysis_from_markdown(markdown: &str) -> Option<Value> {
    fn section(markdown: &str, names: &[&str]) -> String {
        let mut active = false;
        let mut values = Vec::new();
        for line in markdown.lines() {
            if let Some(heading) = line.strip_prefix("## ") {
                if active {
                    break;
                }
                active = names.iter().any(|name| heading.trim() == *name);
                continue;
            }
            if active && !line.trim().is_empty() {
                values.push(line.trim().to_owned());
            }
        }
        values.join("\n").trim().to_owned()
    }

    fn list(markdown: &str, names: &[&str]) -> Vec<String> {
        section(markdown, names)
            .lines()
            .map(|line| line.trim().trim_start_matches("- ").to_owned())
            .filter(|line| !line.is_empty())
            .collect()
    }

    let research_question = section(markdown, &["研究问题", "Research question"]);
    let contribution = section(markdown, &["核心贡献", "Contribution"]);
    let method = section(markdown, &["方法", "Method"]);
    if research_question.is_empty() && contribution.is_empty() && method.is_empty() {
        return None;
    }
    let takeaway = section(markdown, &["一句话结论"]);
    let takeaway = if takeaway.is_empty() {
        contribution
            .lines()
            .next()
            .unwrap_or("尚待重新分析")
            .to_owned()
    } else {
        takeaway
    };
    Some(json!({
        "takeaway": takeaway,
        "research_question": research_question,
        "contribution": contribution,
        "method": method,
        "experimental_design": section(markdown, &["实验设计", "Experimental design"]),
        "baselines": list(markdown, &["对比基线", "Baselines"]),
        "results": list(markdown, &["关键结果", "Results"]),
        "limitations": list(markdown, &["局限", "Limitations"]),
        "assumptions": list(markdown, &["前提与假设", "Assumptions"]),
        "reproducibility": section(markdown, &["可复现性", "Reproducibility"]),
    }))
}

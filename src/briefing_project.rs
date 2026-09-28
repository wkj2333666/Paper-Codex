//! Project-owned search context. Planning precedes retrieval, not just writing.
use crate::{
    codex::{CodexRuntime, CodexTurn},
    db::Database,
    project_readme::ProjectReadmeStore,
    workspace::{atomic_write, Workspace},
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use tokio::sync::watch;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchPlan {
    #[serde(default)]
    pub terms: Vec<String>,
    pub rationale: String,
    pub topics: Vec<SearchTopic>,
    pub dispositions: Vec<ProjectDisposition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchTopic {
    pub id: String,
    pub label: String,
    pub project_ids: Vec<String>,
    pub intent: String,
    pub priority: u8,
    pub terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectDisposition {
    pub project_id: String,
    pub kind: String,
    pub reason: String,
}
impl SearchPlan {
    fn validate(&self) -> Result<()> {
        if self.topics.is_empty()
            || self.topics.len() > 12
            || self.terms.len() > 80
            || self.rationale.len() > 2000
            || self
                .terms
                .iter()
                .chain(self.topics.iter().flat_map(|topic| &topic.terms))
                .any(|term| {
                    term.trim().len() < 3
                        || term.len() > 100
                        || search_words(term).is_empty()
                        || !term
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || " -_.".contains(c))
                })
        {
            bail!("项目检索规划格式无效；请补充项目目的或 README 后重试");
        }
        let mut ids = BTreeSet::new();
        for topic in &self.topics {
            if topic.id.is_empty()
                || topic.id.len() > 64
                || !topic
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                || !ids.insert(&topic.id)
                || topic.label.trim().is_empty()
                || topic.label.len() > 300
                || topic.intent.trim().is_empty()
                || topic.intent.len() > 2000
                || topic.project_ids.is_empty()
                || topic.terms.is_empty()
                || topic.terms.len() > 5
                || !(1..=3).contains(&topic.priority)
            {
                bail!("晨报研究主题缺少明确范围或超出预算");
            }
        }
        if self.dispositions.iter().any(|item| {
            !matches!(
                item.kind.as_str(),
                "organizational" | "clarification" | "deferred"
            ) || item.reason.trim().is_empty()
                || item.reason.len() > 2000
        }) {
            bail!("晨报未检索方向必须说明原因");
        }
        Ok(())
    }

    fn validate_context(&self, context: &Value) -> Result<()> {
        self.validate()?;
        let researched: BTreeSet<_> = self
            .topics
            .iter()
            .flat_map(|topic| topic.project_ids.iter().map(String::as_str))
            .collect();
        let mut disposed = BTreeSet::new();
        if self.dispositions.iter().any(|item| {
            researched.contains(item.project_id.as_str())
                || !disposed.insert(item.project_id.as_str())
        }) {
            bail!("晨报规划对同一节点给出了矛盾或重复的范围说明");
        }
        let known: BTreeSet<_> = context["structure"]
            .as_array()
            .context("项目结构缺失")?
            .iter()
            .filter_map(|node| node["id"].as_str())
            .collect();
        let covered: BTreeSet<_> = self
            .topics
            .iter()
            .flat_map(|topic| topic.project_ids.iter().map(String::as_str))
            .chain(
                self.dispositions
                    .iter()
                    .map(|item| item.project_id.as_str()),
            )
            .collect();
        if known != covered {
            bail!("晨报规划遗漏项目节点或引用了不存在的节点；未开始检索");
        }
        Ok(())
    }

    pub fn query(&self, categories: &[String]) -> String {
        Self::query_terms(categories, &self.terms)
    }

    pub fn query_terms(categories: &[String], search_terms: &[String]) -> String {
        let categories = categories
            .iter()
            .map(|category| format!("cat:{category}"))
            .collect::<Vec<_>>()
            .join(" OR ");
        let terms = search_terms
            .iter()
            .map(|term| {
                let words = search_words(term)
                    .into_iter()
                    .map(|word| format!("(ti:{word} OR abs:{word})"))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                format!("({words})")
            })
            .collect::<Vec<_>>()
            .join(" OR ");
        format!("({categories}) AND ({terms})")
    }

    pub fn normalized(mut self) -> Self {
        self.terms = self
            .topics
            .iter()
            .flat_map(|topic| topic.terms.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self
    }
}

pub(crate) fn search_words(term: &str) -> Vec<String> {
    term.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| {
            !word.is_empty()
                && !matches!(
                    *word,
                    "a" | "an" | "the" | "for" | "of" | "in" | "and" | "with" | "to"
                )
        })
        .map(str::to_owned)
        .collect()
}

pub(crate) async fn context(
    db: &Database,
    workspace: &Workspace,
    project_id: &str,
) -> Result<Value> {
    let root = db
        .get_project(project_id)
        .await?
        .context("晨报所属项目不存在")?;
    let projects = db.list_projects().await?;
    let mut ids = BTreeSet::from([project_id.to_owned()]);
    loop {
        let before = ids.len();
        for project in &projects {
            if project
                .parent_id
                .as_ref()
                .is_some_and(|parent| ids.contains(parent))
            {
                ids.insert(project.id.clone());
            }
        }
        if ids.len() == before {
            break;
        }
    }
    let tree: Vec<_> = projects.iter().filter(|project| ids.contains(&project.id))
        .map(|project| json!({"id":project.id,"name":project.name,"purpose":project.purpose.chars().take(2000).collect::<String>(),"parent_id":project.parent_id})).collect();
    let readmes = ProjectReadmeStore::new(db.clone(), workspace.clone());
    let readme = readmes.read(project_id).await?;
    let mut branch_readmes = Vec::new();
    for id in ids.iter().filter(|id| id.as_str() != project_id).take(12) {
        let readme = readmes.read(id).await?;
        branch_readmes.push(json!({"project_id":id,"readme":readme.markdown.chars().take(2000).collect::<String>()}));
    }
    let mut ancestors = Vec::new();
    let mut parent = root.parent_id.as_deref();
    let mut visited = BTreeSet::new();
    while let Some(id) = parent {
        if ancestors.len() >= 8 || !visited.insert(id) {
            break;
        }
        let Some(project) = projects.iter().find(|project| project.id == id) else {
            break;
        };
        ancestors.push(json!({"name":project.name,"purpose":project.purpose.chars().take(2000).collect::<String>()}));
        parent = project.parent_id.as_deref();
    }
    let memories = db
        .list_memory_items("project", Some(project_id), &[])
        .await?;
    let goals = db.project_goal_summaries(project_id).await?;
    let mut papers = Vec::new();
    let mut paper_ids = BTreeSet::new();
    for id in ids.iter().take(60) {
        for paper_id in db.project_paper_ids(id).await? {
            if papers.len() >= 80 {
                break;
            }
            if paper_ids.insert(paper_id.clone()) {
                if let Some(paper) = db.get_paper(&paper_id).await? {
                    if paper.deleted_at.is_none() {
                        papers.push(json!({"id":paper.id,"title":paper.title,"project_id":id}));
                    }
                }
            }
        }
    }
    Ok(
        json!({"project":{"id":root.id,"name":root.name,"purpose":root.purpose},
        "readme":readme.markdown.chars().take(12000).collect::<String>(), "structure":tree,"branch_readmes":branch_readmes,"parent_context":ancestors,
        "context_limits":{"branch_readmes_loaded":branch_readmes.len(),"descendant_count":ids.len().saturating_sub(1),"readmes_may_be_partial":ids.len()>13},
        "goals":goals.into_iter().take(12).collect::<Vec<_>>(),
        "memories":memories.into_iter().take(12).collect::<Vec<_>>(),"existing_papers":papers}),
    )
}

pub(crate) async fn plan(
    codex: &CodexRuntime,
    workspace: &Workspace,
    context: &Value,
    keywords: &[String],
    cancel: watch::Receiver<bool>,
) -> Result<SearchPlan> {
    let skill = crate::briefing_skill::instructions(workspace, true).await?;
    let input = json!({"project":context,"additional_interests":keywords});
    let key = hex::encode(Sha256::digest(format!(
        "project-briefing-search-v2:{skill}:{input}"
    )));
    let cache = workspace
        .state_dir()
        .join("briefing-search")
        .join(format!("{key}.json"));
    if let Ok(bytes) = tokio::fs::read(&cache).await {
        if let Ok(plan) = serde_json::from_slice::<SearchPlan>(&bytes) {
            if plan.validate_context(context).is_ok() {
                return Ok(plan);
            }
        }
    }
    let prompt = format!(
        r#"{skill}

当前模式：检索规划。只返回上述 schema 的 JSON。全部 structure 节点都须映射到 topics.project_ids 或 dispositions，包括组织性父节点。
输入：{input}"#
    );
    let cwd = workspace.state_dir().join("briefing-work");
    tokio::fs::create_dir_all(&cwd).await?;
    let result = codex
        .run_turn(
            CodexTurn {
                thread_id: None,
                cwd,
                prompt,
                skill: Some(crate::briefing_skill::selection(workspace)),
                tool_preferences: vec![],
                output_schema: None,
                settings: codex.research_conversation_settings(),
            },
            cancel,
        )
        .await?;
    if result.status != "completed" {
        bail!("项目检索规划未完成；请检查模型后手动重试");
    }
    let text = result
        .final_text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let result: SearchPlan = serde_json::from_str(text).context("项目检索规划不是有效 JSON")?;
    result.validate_context(context)?;
    if let Some(parent) = cache.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    atomic_write(&cache, &serde_json::to_vec(&result)?).await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn context_includes_owned_tree_and_readmes_not_unrelated_projects() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = Workspace::initialize(directory.path()).await.unwrap();
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let root = db
            .create_project("ei", "EI", "Embodied research")
            .await
            .unwrap();
        let child = db
            .create_project_with_parent("vla", "VLA", "Action representations", Some(&root))
            .await
            .unwrap();
        db.create_project("unrelated", "Unrelated", "DO_NOT_INCLUDE_THIS")
            .await
            .unwrap();
        atomic_write(
            &directory.path().join("projects/vla/README.md"),
            b"# Child detailed goals",
        )
        .await
        .unwrap();
        let value = context(&db, &workspace, &root).await.unwrap();
        assert_eq!(value["project"]["id"], root);
        assert_eq!(value["structure"].as_array().unwrap().len(), 2);
        assert!(value.to_string().contains("Child detailed goals"));
        assert!(!value.to_string().contains("DO_NOT_INCLUDE_THIS"));
        let value = context(&db, &workspace, &child).await.unwrap();
        assert_eq!(value["structure"].as_array().unwrap().len(), 1);
        assert_eq!(value["parent_context"][0]["name"], "EI");
    }
    #[test]
    fn scoped_query_requires_project_terms_and_cannot_inject_query_syntax() {
        let plan = SearchPlan {
            terms: vec!["point cloud".into(), "vision-language-action".into()],
            rationale: "项目目的和子方向".into(),
            topics: vec![SearchTopic {
                id: "representations".into(),
                label: "表征".into(),
                project_ids: vec!["a".into()],
                intent: "Research representations".into(),
                priority: 2,
                terms: vec!["point cloud".into(), "vision-language-action".into()],
            }],
            dispositions: vec![],
        };
        plan.validate().unwrap();
        let query = plan.query(&["cs.RO".into()]);
        assert!(query.starts_with("(cat:cs.RO) AND"));
        assert!(query.contains("(ti:point OR abs:point) AND (ti:cloud OR abs:cloud)"));
        assert!(query.contains("(ti:vision OR abs:vision) AND (ti:language OR abs:language) AND (ti:action OR abs:action)"));
        assert!(!query.contains('"'));
        let expanded = plan.clone().normalized();
        assert_eq!(expanded.terms.len(), 2);
        for term in ["x\" OR all:*", "", "机器人", "\nrobot", "for the"] {
            assert!(SearchPlan {
                terms: vec![term.into()],
                ..plan.clone()
            }
            .validate()
            .is_err());
        }
    }

    #[test]
    fn semantic_groups_can_cross_tree_levels_but_cannot_silently_omit_nodes() {
        let context = json!({"structure":[{"id":"root"},{"id":"method"},{"id":"nested"},{"id":"benchmark"},{"id":"unclear"}]});
        let mut plan = SearchPlan {
            terms: vec![],
            rationale: "One question spans method and evaluation folders".into(),
            topics: vec![SearchTopic {
                id: "robustness".into(),
                label: "Robustness".into(),
                project_ids: vec!["method".into(), "nested".into(), "benchmark".into()],
                intent: "Robustness across implementations and evaluation".into(),
                priority: 2,
                terms: vec!["robust control".into()],
            }],
            dispositions: vec![
                ProjectDisposition {
                    project_id: "root".into(),
                    kind: "organizational".into(),
                    reason: "Groups research".into(),
                },
                ProjectDisposition {
                    project_id: "unclear".into(),
                    kind: "clarification".into(),
                    reason: "Acronym has no definition".into(),
                },
            ],
        };
        plan.validate_context(&context).unwrap();
        let mut contradictory = plan.clone();
        contradictory.dispositions.push(ProjectDisposition {
            project_id: "method".into(),
            kind: "deferred".into(),
            reason: "Cannot be researched and deferred at once".into(),
        });
        assert!(contradictory.validate_context(&context).is_err());
        plan.dispositions.pop();
        assert!(plan.validate_context(&context).is_err());
        plan.topics[0].project_ids.push("made-up".into());
        assert!(plan.validate_context(&context).is_err());
    }
}

use anyhow::{bail, Result};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum EvidenceLevel {
    Metadata,
    Abstract,
    Fulltext,
}

impl EvidenceLevel {
    pub fn strongest(self, other: Self) -> Self {
        if self.rank() >= other.rank() {
            self
        } else {
            other
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Metadata => 0,
            Self::Abstract => 1,
            Self::Fulltext => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum CandidateStatus {
    Candidate,
    Importing,
    Imported,
    Dismissed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ResearchMode {
    Auto,
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ResearchTrigger {
    Automatic,
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum SearchRunState {
    Running,
    Completed,
    Partial,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FulltextState {
    Available,
    Possible,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryMatch {
    pub score: f64,
    pub title_exact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryFulltext {
    pub state: FulltextState,
    pub source_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryResult {
    pub work: DiscoveredWork,
    pub providers: Vec<String>,
    pub best_rank: Option<i64>,
    pub provider_scores: serde_json::Value,
    pub raw_results: Vec<serde_json::Value>,
    #[serde(rename = "match")]
    pub match_info: DiscoveryMatch,
    pub fulltext: DiscoveryFulltext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkMetadata {
    pub canonical_key: String,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub openalex_id: Option<String>,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i64>,
    pub abstract_text: Option<String>,
    pub source_url: String,
    pub pdf_url: Option<String>,
    pub evidence_level: EvidenceLevel,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredWork {
    pub id: String,
    #[serde(flatten)]
    pub metadata: WorkMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiteratureSearchRun {
    pub id: String,
    pub project_id: String,
    pub conversation_id: String,
    pub message_id: String,
    pub trigger: ResearchTrigger,
    pub question: String,
    pub query_plan: serde_json::Value,
    pub state: SearchRunState,
    pub provider_status: serde_json::Value,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiteratureSearchResult {
    pub search_run_id: String,
    pub work: DiscoveredWork,
    pub providers: Vec<String>,
    pub best_rank: Option<i64>,
    pub provider_scores: serde_json::Value,
    pub raw_results: Vec<serde_json::Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectCandidate {
    pub project_id: String,
    pub work: DiscoveredWork,
    pub status: CandidateStatus,
    pub relevance_reason: String,
    pub relevance_tags: Vec<String>,
    pub evidence_level: EvidenceLevel,
    pub discovered_by_search_run_id: Option<String>,
    pub discovered_by_conversation_id: Option<String>,
    pub import_task_id: Option<String>,
    pub paper_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateSource {
    pub work_id: String,
    pub title: String,
    pub source_url: String,
    pub evidence_level: EvidenceLevel,
    pub abstract_text: Option<String>,
    pub pdf_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct MessageCandidateCitation {
    pub id: String,
    pub message_id: String,
    pub project_id: String,
    pub work_id: String,
    pub title: String,
    pub source_url: String,
    pub evidence_level: EvidenceLevel,
    pub quote: String,
    pub explanation: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PossibleDuplicate {
    pub left_key: String,
    pub right_key: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchQuery {
    pub text: String,
    pub title_terms: Vec<String>,
    pub author: Option<String>,
    pub year_from: Option<i64>,
    pub year_to: Option<i64>,
    pub limit: usize,
}

impl ResearchQuery {
    pub fn normalized(&self) -> Result<Self> {
        let text = self.text.trim();
        if text.is_empty() {
            bail!("research query cannot be empty");
        }
        if self
            .year_from
            .zip(self.year_to)
            .is_some_and(|(from, to)| from > to)
        {
            bail!("research query start year cannot exceed end year");
        }
        Ok(Self {
            text: text.to_owned(),
            title_terms: self
                .title_terms
                .iter()
                .map(|term| term.trim())
                .filter(|term| !term.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
            author: self
                .author
                .as_deref()
                .map(str::trim)
                .filter(|author| !author.is_empty())
                .map(ToOwned::to_owned),
            year_from: self.year_from,
            year_to: self.year_to,
            limit: self.limit.clamp(1, 50),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderSearchResult {
    pub provider: &'static str,
    pub works: Vec<WorkMetadata>,
}

#[async_trait]
pub trait ResearchProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn search(&self, query: &ResearchQuery) -> Result<Vec<WorkMetadata>>;
}

pub fn canonical_doi(value: &str) -> Option<String> {
    let mut normalized = value.trim().to_ascii_lowercase();
    for prefix in [
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
        "http://dx.doi.org/",
        "doi:",
    ] {
        if let Some(rest) = normalized.strip_prefix(prefix) {
            normalized = rest.trim().to_owned();
            break;
        }
    }
    normalized = normalized
        .trim_end_matches(['.', ',', ';', ')', ']', '}'])
        .to_owned();
    (normalized.starts_with("10.") && normalized.contains('/')).then_some(normalized)
}

pub fn canonical_arxiv_id(value: &str) -> Option<String> {
    let mut normalized = value.trim().to_ascii_lowercase();
    for prefix in [
        "https://arxiv.org/abs/",
        "http://arxiv.org/abs/",
        "https://arxiv.org/pdf/",
        "http://arxiv.org/pdf/",
        "arxiv:",
    ] {
        if let Some(rest) = normalized.strip_prefix(prefix) {
            normalized = rest.trim().to_owned();
            break;
        }
    }
    if let Some(rest) = normalized.strip_suffix(".pdf") {
        normalized = rest.to_owned();
    }
    if let Some(version_start) = normalized.rfind('v') {
        let suffix = &normalized[version_start + 1..];
        if !suffix.is_empty() && suffix.chars().all(|character| character.is_ascii_digit()) {
            normalized.truncate(version_start);
        }
    }
    let valid = normalized
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "/.-".contains(character))
        && (normalized.contains('.') || normalized.contains('/'));
    valid.then_some(normalized)
}

pub fn canonical_openalex_id(value: &str) -> Option<String> {
    let normalized = value
        .trim()
        .trim_start_matches("https://openalex.org/")
        .trim_start_matches("http://openalex.org/")
        .trim_start_matches("openalex:")
        .trim();
    (!normalized.is_empty()).then(|| normalized.to_ascii_uppercase())
}

pub fn canonical_key(
    doi: Option<&str>,
    arxiv_id: Option<&str>,
    openalex_id: Option<&str>,
) -> Option<String> {
    if let Some(doi) = doi.and_then(canonical_doi) {
        return Some(format!("doi:{doi}"));
    }
    if let Some(arxiv_id) = arxiv_id.and_then(canonical_arxiv_id) {
        return Some(format!("arxiv:{arxiv_id}"));
    }
    openalex_id
        .and_then(canonical_openalex_id)
        .map(|openalex_id| format!("openalex:{openalex_id}"))
}

pub fn possible_title_duplicate(
    left: &DiscoveredWork,
    right: &DiscoveredWork,
) -> Option<PossibleDuplicate> {
    let normalized_title = |title: &str| {
        title
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    };
    let left_author = left.metadata.authors.first().map(|author| {
        author
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    });
    let right_author = right.metadata.authors.first().map(|author| {
        author
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    });
    (left.metadata.canonical_key != right.metadata.canonical_key
        && normalized_title(&left.metadata.title) == normalized_title(&right.metadata.title)
        && left.metadata.year == right.metadata.year
        && left_author.is_some()
        && left_author == right_author)
        .then(|| PossibleDuplicate {
            left_key: left.metadata.canonical_key.clone(),
            right_key: right.metadata.canonical_key.clone(),
            reason: "标题、第一作者和年份相同，但缺少可安全合并的标识符".to_owned(),
        })
}

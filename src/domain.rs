use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{fmt, sync::OnceLock};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PaperIdentity {
    Doi(String),
    Arxiv(String),
    Sha256(String),
}

impl PaperIdentity {
    pub fn parse(input: &str) -> Option<Self> {
        static DOI: OnceLock<Regex> = OnceLock::new();
        static ARXIV_DOI: OnceLock<Regex> = OnceLock::new();
        static ARXIV: OnceLock<Regex> = OnceLock::new();
        let lower = input.trim().to_ascii_lowercase();
        let arxiv_doi_re = ARXIV_DOI.get_or_init(|| {
            Regex::new(
                r"(?i)10\.48550/arxiv\.((?:\d{4}\.\d{4,5}|[a-z-]+/\d{7}))(?:v\d+)?(?:$|[?#.,;)\s])",
            )
            .unwrap()
        });
        if let Some(capture) = arxiv_doi_re.captures(&lower) {
            return Some(Self::Arxiv(capture.get(1)?.as_str().to_string()));
        }
        let doi_re = DOI.get_or_init(|| {
            Regex::new(
                r"(?i)(?:doi:\s*|https?://(?:dx\.)?doi\.org/)?(10\.\d{4,9}/[-._;()/:a-z0-9]+)",
            )
            .unwrap()
        });
        if let Some(capture) = doi_re.captures(&lower) {
            let doi = capture
                .get(1)?
                .as_str()
                .trim_end_matches(['.', ',', ';'])
                .to_string();
            return Some(Self::Doi(doi));
        }
        let arxiv_re = ARXIV.get_or_init(|| {
            Regex::new(r"(?i)(?:arxiv:\s*|https?://arxiv\.org/(?:abs|pdf)/)?((?:\d{4}\.\d{4,5}|[a-z-]+/\d{7}))(?:v\d+)?(?:\.pdf)?(?:[?#].*)?$").unwrap()
        });
        if let Some(capture) = arxiv_re.captures(&lower) {
            return Some(Self::Arxiv(capture.get(1)?.as_str().to_string()));
        }
        lower
            .strip_prefix("sha256:")
            .filter(|hash| hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .map(|hash| Self::Sha256(hash.to_string()))
    }

    pub fn file_key(&self) -> String {
        self.to_string().replace([':', '/'], "_")
    }
}

impl fmt::Display for PaperIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Doi(value) => write!(f, "doi:{value}"),
            Self::Arxiv(value) => write!(f, "arxiv:{value}"),
            Self::Sha256(value) => write!(f, "sha256:{value}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    Queued,
    Resolving,
    Fetching,
    Extracting,
    Analyzing,
    Staging,
    Validating,
    Committing,
    Indexing,
    Done,
    NeedsInput,
    Cancelled,
    Failed,
}

impl TaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Resolving => "resolving",
            Self::Fetching => "fetching",
            Self::Extracting => "extracting",
            Self::Analyzing => "analyzing",
            Self::Staging => "staging",
            Self::Validating => "validating",
            Self::Committing => "committing",
            Self::Indexing => "indexing",
            Self::Done => "done",
            Self::NeedsInput => "needs-input",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        use TaskState::*;
        if matches!(next, Failed | Cancelled) {
            return !matches!(self, Done | Cancelled | Failed);
        }
        matches!(
            (self, next),
            (Queued, Resolving)
                | (Resolving, Fetching | NeedsInput)
                | (NeedsInput, Resolving)
                | (Fetching, Extracting)
                | (Extracting, Analyzing)
                | (Analyzing, Staging)
                | (Staging, Validating)
                | (Validating, Committing | Analyzing)
                | (Committing, Indexing)
                | (Indexing, Done)
        )
    }
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for TaskState {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "resolving" => Ok(Self::Resolving),
            "fetching" => Ok(Self::Fetching),
            "extracting" => Ok(Self::Extracting),
            "analyzing" => Ok(Self::Analyzing),
            "staging" => Ok(Self::Staging),
            "validating" => Ok(Self::Validating),
            "committing" => Ok(Self::Committing),
            "indexing" => Ok(Self::Indexing),
            "done" => Ok(Self::Done),
            "needs-input" => Ok(Self::NeedsInput),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            other => Err(format!("unknown task state: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Paper {
    pub id: String,
    pub title: String,
    pub authors_json: String,
    pub year: Option<i64>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub canonical_sha256: Option<String>,
    pub source_url: Option<String>,
    pub note_path: Option<String>,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub purpose: String,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Task {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub input_json: String,
    pub paper_id: Option<String>,
    pub project_id: Option<String>,
    pub thread_id: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskEvent {
    pub id: i64,
    pub task_id: String,
    pub event_type: String,
    pub payload_json: String,
    pub created_at: String,
}

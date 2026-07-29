use crate::{
    db::Database,
    research::{
        canonical_arxiv_id, canonical_doi, canonical_key, canonical_openalex_id, CandidateSource,
        CandidateStatus, DiscoveredWork, LiteratureSearchResult, LiteratureSearchRun,
        ProjectCandidate, ResearchTrigger, SearchRunState, WorkMetadata,
    },
};
use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde_json::Value;
use sqlx::{sqlite::SqliteRow, Row};
use uuid::Uuid;

#[derive(Clone)]
pub struct ResearchStore {
    db: Database,
}

impl ResearchStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn database(&self) -> &Database {
        &self.db
    }

    pub async fn upsert_work(&self, work: WorkMetadata) -> Result<DiscoveredWork> {
        let work = normalize_work(work)?;
        let existing_id = self.find_work_id(&work).await?;
        let id = existing_id.unwrap_or_else(|| Uuid::new_v4().to_string());

        if self.get_work(&id).await?.is_some() {
            let current = self
                .get_work(&id)
                .await?
                .context("discovered work disappeared during update")?;
            let evidence_level = current
                .metadata
                .evidence_level
                .strongest(work.evidence_level);
            let authors = if work.authors.is_empty() {
                current.metadata.authors
            } else {
                work.authors
            };
            let abstract_text = work.abstract_text.or(current.metadata.abstract_text);
            let pdf_url = work.pdf_url.or(current.metadata.pdf_url);
            let metadata = merge_json(current.metadata.metadata, work.metadata);
            sqlx::query(
                r#"UPDATE discovered_works SET
                   canonical_key=?,doi=?,arxiv_id=?,openalex_id=?,title=?,authors_json=?,year=?,
                   abstract_text=?,source_url=?,pdf_url=?,evidence_level=?,metadata_json=?,
                   refreshed_at=CURRENT_TIMESTAMP
                   WHERE id=?"#,
            )
            .bind(&work.canonical_key)
            .bind(&work.doi)
            .bind(&work.arxiv_id)
            .bind(&work.openalex_id)
            .bind(&work.title)
            .bind(serde_json::to_string(&authors)?)
            .bind(work.year.or(current.metadata.year))
            .bind(abstract_text)
            .bind(&work.source_url)
            .bind(pdf_url)
            .bind(evidence_level)
            .bind(serde_json::to_string(&metadata)?)
            .bind(&id)
            .execute(self.db.pool())
            .await?;
        } else {
            sqlx::query(
                r#"INSERT INTO discovered_works(
                   id,canonical_key,doi,arxiv_id,openalex_id,title,authors_json,year,
                   abstract_text,source_url,pdf_url,evidence_level,metadata_json
                   ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)"#,
            )
            .bind(&id)
            .bind(&work.canonical_key)
            .bind(&work.doi)
            .bind(&work.arxiv_id)
            .bind(&work.openalex_id)
            .bind(&work.title)
            .bind(serde_json::to_string(&work.authors)?)
            .bind(work.year)
            .bind(&work.abstract_text)
            .bind(&work.source_url)
            .bind(&work.pdf_url)
            .bind(work.evidence_level)
            .bind(serde_json::to_string(&work.metadata)?)
            .execute(self.db.pool())
            .await?;
        }

        self.get_work(&id)
            .await?
            .context("upserted discovered work is missing")
    }

    pub async fn get_work(&self, id: &str) -> Result<Option<DiscoveredWork>> {
        let row = sqlx::query(
            r#"SELECT id,canonical_key,doi,arxiv_id,openalex_id,title,authors_json,year,
               abstract_text,source_url,pdf_url,evidence_level,metadata_json
               FROM discovered_works WHERE id=?"#,
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await?;
        row.as_ref().map(work_from_row).transpose()
    }

    pub async fn start_search(
        &self,
        project_id: &str,
        conversation_id: &str,
        message_id: &str,
        trigger: ResearchTrigger,
        question: &str,
    ) -> Result<LiteratureSearchRun> {
        self.require_project(project_id).await?;
        if question.trim().is_empty() {
            bail!("research question cannot be empty");
        }
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"INSERT INTO literature_search_runs(
               id,project_id,conversation_id,message_id,trigger_type,question,state
               ) VALUES(?,?,?,?,?,?,'running')"#,
        )
        .bind(&id)
        .bind(project_id)
        .bind(conversation_id)
        .bind(message_id)
        .bind(trigger)
        .bind(question.trim())
        .execute(self.db.pool())
        .await?;
        self.get_search(&id)
            .await?
            .context("created literature search is missing")
    }

    pub async fn finish_search(
        &self,
        run_id: &str,
        state: SearchRunState,
        provider_status: &Value,
        error: Option<&str>,
    ) -> Result<()> {
        if state == SearchRunState::Running {
            bail!("finished search cannot remain running");
        }
        let changed = sqlx::query(
            r#"UPDATE literature_search_runs
               SET state=?,provider_status_json=?,error=?,updated_at=CURRENT_TIMESTAMP
               WHERE id=?"#,
        )
        .bind(state)
        .bind(serde_json::to_string(provider_status)?)
        .bind(error)
        .bind(run_id)
        .execute(self.db.pool())
        .await?
        .rows_affected();
        if changed == 0 {
            bail!("literature search does not exist");
        }
        Ok(())
    }

    pub async fn get_search(&self, run_id: &str) -> Result<Option<LiteratureSearchRun>> {
        let row = sqlx::query(
            r#"SELECT id,project_id,conversation_id,message_id,trigger_type,question,
               query_plan_json,state,provider_status_json,error,created_at,updated_at
               FROM literature_search_runs WHERE id=?"#,
        )
        .bind(run_id)
        .fetch_optional(self.db.pool())
        .await?;
        row.as_ref().map(search_from_row).transpose()
    }

    pub async fn list_project_searches(
        &self,
        project_id: &str,
    ) -> Result<Vec<LiteratureSearchRun>> {
        self.require_project(project_id).await?;
        let rows = sqlx::query(
            r#"SELECT id,project_id,conversation_id,message_id,trigger_type,question,
               query_plan_json,state,provider_status_json,error,created_at,updated_at
               FROM literature_search_runs WHERE project_id=?
               ORDER BY created_at DESC,id"#,
        )
        .bind(project_id)
        .fetch_all(self.db.pool())
        .await?;
        rows.iter().map(search_from_row).collect()
    }

    pub async fn save_search_results(
        &self,
        run_id: &str,
        provider: &str,
        works: &[DiscoveredWork],
    ) -> Result<()> {
        if provider.trim().is_empty() {
            bail!("research provider cannot be empty");
        }
        if self.get_search(run_id).await?.is_none() {
            bail!("literature search does not exist");
        }
        let mut transaction = self.db.pool().begin().await?;
        for (rank, work) in works.iter().enumerate() {
            let existing = sqlx::query(
                r#"SELECT providers_json,best_rank,raw_results_json
                   FROM literature_search_results
                   WHERE search_run_id=? AND work_id=?"#,
            )
            .bind(run_id)
            .bind(&work.id)
            .fetch_optional(&mut *transaction)
            .await?;
            let (mut providers, best_rank, mut raw_results) = if let Some(row) = existing {
                (
                    json_column::<Vec<String>>(&row, "providers_json")?,
                    row.try_get::<Option<i64>, _>("best_rank")?,
                    json_column::<Vec<Value>>(&row, "raw_results_json")?,
                )
            } else {
                (Vec::new(), None, Vec::new())
            };
            if !providers.iter().any(|value| value == provider) {
                providers.push(provider.to_owned());
            }
            raw_results.push(work.metadata.metadata.clone());
            let rank = i64::try_from(rank + 1).context("provider result rank exceeds i64")?;
            let best_rank = Some(best_rank.map_or(rank, |current| current.min(rank)));
            sqlx::query(
                r#"INSERT INTO literature_search_results(
                   search_run_id,work_id,providers_json,best_rank,raw_results_json
                   ) VALUES(?,?,?,?,?)
                   ON CONFLICT(search_run_id,work_id) DO UPDATE SET
                   providers_json=excluded.providers_json,
                   best_rank=excluded.best_rank,
                   raw_results_json=excluded.raw_results_json"#,
            )
            .bind(run_id)
            .bind(&work.id)
            .bind(serde_json::to_string(&providers)?)
            .bind(best_rank)
            .bind(serde_json::to_string(&raw_results)?)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn search_results(&self, run_id: &str) -> Result<Vec<LiteratureSearchResult>> {
        let rows = sqlx::query(
            r#"SELECT
               r.search_run_id,r.providers_json,r.best_rank,r.provider_scores_json,
               r.raw_results_json,r.created_at,
               w.id AS work_id,w.canonical_key,w.doi,w.arxiv_id,w.openalex_id,w.title,
               w.authors_json,w.year,w.abstract_text,w.source_url,w.pdf_url,
               w.evidence_level,w.metadata_json
               FROM literature_search_results r
               JOIN discovered_works w ON w.id=r.work_id
               WHERE r.search_run_id=?
               ORDER BY r.best_rank,w.id"#,
        )
        .bind(run_id)
        .fetch_all(self.db.pool())
        .await?;
        rows.iter().map(search_result_from_row).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn save_candidate(
        &self,
        project_id: &str,
        work_id: &str,
        reason: &str,
        tags: &[String],
        run_id: Option<&str>,
        conversation_id: Option<&str>,
    ) -> Result<ProjectCandidate> {
        self.require_project(project_id).await?;
        let work = self
            .get_work(work_id)
            .await?
            .context("discovered work does not exist")?;
        if reason.trim().is_empty() {
            bail!("candidate relevance reason cannot be empty");
        }
        sqlx::query(
            r#"INSERT INTO project_candidates(
               project_id,work_id,status,relevance_reason,relevance_tags_json,evidence_level,
               discovered_by_search_run_id,discovered_by_conversation_id
               ) VALUES(?,?,'candidate',?,?,?,?,?)
               ON CONFLICT(project_id,work_id) DO UPDATE SET
               status='candidate',
               relevance_reason=excluded.relevance_reason,
               relevance_tags_json=excluded.relevance_tags_json,
               evidence_level=excluded.evidence_level,
               discovered_by_search_run_id=COALESCE(
                 excluded.discovered_by_search_run_id,
                 project_candidates.discovered_by_search_run_id
               ),
               discovered_by_conversation_id=COALESCE(
                 excluded.discovered_by_conversation_id,
                 project_candidates.discovered_by_conversation_id
               ),
               updated_at=CURRENT_TIMESTAMP"#,
        )
        .bind(project_id)
        .bind(work_id)
        .bind(reason.trim())
        .bind(serde_json::to_string(tags)?)
        .bind(work.metadata.evidence_level)
        .bind(run_id)
        .bind(conversation_id)
        .execute(self.db.pool())
        .await?;
        self.get_candidate(project_id, work_id)
            .await?
            .context("saved project candidate is missing")
    }

    pub async fn get_candidate(
        &self,
        project_id: &str,
        work_id: &str,
    ) -> Result<Option<ProjectCandidate>> {
        let query = format!("{CANDIDATE_COLUMNS} WHERE c.project_id=? AND c.work_id=?");
        let row = sqlx::query(&query)
            .bind(project_id)
            .bind(work_id)
            .fetch_optional(self.db.pool())
            .await?;
        row.as_ref().map(candidate_from_row).transpose()
    }

    pub async fn set_candidate_status(
        &self,
        project_id: &str,
        work_id: &str,
        status: CandidateStatus,
    ) -> Result<ProjectCandidate> {
        self.require_project(project_id).await?;
        let changed = sqlx::query(
            r#"UPDATE project_candidates
               SET status=?,updated_at=CURRENT_TIMESTAMP
               WHERE project_id=? AND work_id=?"#,
        )
        .bind(status)
        .bind(project_id)
        .bind(work_id)
        .execute(self.db.pool())
        .await?
        .rows_affected();
        if changed == 0 {
            bail!("project candidate does not exist");
        }
        self.get_candidate(project_id, work_id)
            .await?
            .context("updated project candidate is missing")
    }

    pub async fn remove_candidate(&self, project_id: &str, work_id: &str) -> Result<()> {
        self.require_project(project_id).await?;
        let candidate = self
            .get_candidate(project_id, work_id)
            .await?
            .context("project candidate does not exist")?;
        if candidate.status == CandidateStatus::Importing {
            bail!("cannot remove a candidate while it is importing");
        }
        sqlx::query("DELETE FROM project_candidates WHERE project_id=? AND work_id=?")
            .bind(project_id)
            .bind(work_id)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    pub async fn list_project_candidates(
        &self,
        project_id: &str,
        include_dismissed: bool,
    ) -> Result<Vec<ProjectCandidate>> {
        self.require_project(project_id).await?;
        let filter = if include_dismissed {
            " WHERE c.project_id=? ORDER BY c.updated_at DESC,c.work_id"
        } else {
            " WHERE c.project_id=? AND c.status!='dismissed' ORDER BY c.updated_at DESC,c.work_id"
        };
        let query = format!("{CANDIDATE_COLUMNS}{filter}");
        let rows = sqlx::query(&query)
            .bind(project_id)
            .fetch_all(self.db.pool())
            .await?;
        rows.iter().map(candidate_from_row).collect()
    }

    pub async fn candidate_source(
        &self,
        project_id: &str,
        work_id: &str,
    ) -> Result<Option<CandidateSource>> {
        Ok(self
            .get_candidate(project_id, work_id)
            .await?
            .map(|candidate| CandidateSource {
                work_id: candidate.work.id,
                title: candidate.work.metadata.title,
                source_url: candidate.work.metadata.source_url,
                evidence_level: candidate.evidence_level,
                abstract_text: candidate.work.metadata.abstract_text,
                pdf_url: candidate.work.metadata.pdf_url,
            }))
    }

    async fn require_project(&self, project_id: &str) -> Result<()> {
        if self.db.get_project(project_id).await?.is_none() {
            bail!("project does not exist");
        }
        Ok(())
    }

    async fn find_work_id(&self, work: &WorkMetadata) -> Result<Option<String>> {
        if let Some(id) =
            sqlx::query_scalar("SELECT id FROM discovered_works WHERE canonical_key=?")
                .bind(&work.canonical_key)
                .fetch_optional(self.db.pool())
                .await?
        {
            return Ok(Some(id));
        }
        for (column, value) in [
            ("doi", work.doi.as_deref()),
            ("arxiv_id", work.arxiv_id.as_deref()),
            ("openalex_id", work.openalex_id.as_deref()),
        ] {
            if let Some(value) = value {
                let query = format!("SELECT id FROM discovered_works WHERE {column}=?");
                if let Some(id) = sqlx::query_scalar(&query)
                    .bind(value)
                    .fetch_optional(self.db.pool())
                    .await?
                {
                    return Ok(Some(id));
                }
            }
        }
        Ok(None)
    }
}

const CANDIDATE_COLUMNS: &str = r#"
SELECT
  c.project_id,c.status AS candidate_status,c.relevance_reason,c.relevance_tags_json,
  c.evidence_level AS candidate_evidence_level,c.discovered_by_search_run_id,
  c.discovered_by_conversation_id,c.import_task_id,c.paper_id,c.created_at,c.updated_at,
  w.id AS work_id,w.canonical_key,w.doi,w.arxiv_id,w.openalex_id,w.title,w.authors_json,
  w.year,w.abstract_text,w.source_url,w.pdf_url,w.evidence_level,w.metadata_json
FROM project_candidates c
JOIN discovered_works w ON w.id=c.work_id
"#;

fn normalize_work(mut work: WorkMetadata) -> Result<WorkMetadata> {
    work.title = work.title.trim().to_owned();
    work.source_url = work.source_url.trim().to_owned();
    if work.title.is_empty() {
        bail!("discovered work title cannot be empty");
    }
    if work.source_url.is_empty() {
        bail!("discovered work source URL cannot be empty");
    }
    work.doi = work.doi.as_deref().and_then(canonical_doi).or_else(|| {
        work.canonical_key
            .strip_prefix("doi:")
            .and_then(canonical_doi)
    });
    work.arxiv_id = work
        .arxiv_id
        .as_deref()
        .and_then(canonical_arxiv_id)
        .or_else(|| {
            work.canonical_key
                .strip_prefix("arxiv:")
                .and_then(canonical_arxiv_id)
        });
    work.openalex_id = work
        .openalex_id
        .as_deref()
        .and_then(canonical_openalex_id)
        .or_else(|| {
            work.canonical_key
                .strip_prefix("openalex:")
                .and_then(canonical_openalex_id)
        });
    work.canonical_key = canonical_key(
        work.doi.as_deref(),
        work.arxiv_id.as_deref(),
        work.openalex_id.as_deref(),
    )
    .context("discovered work requires DOI, arXiv ID, or OpenAlex ID")?;
    if work.abstract_text.as_deref().is_some_and(str::is_empty) {
        work.abstract_text = None;
    }
    Ok(work)
}

fn merge_json(current: Value, incoming: Value) -> Value {
    match (current, incoming) {
        (Value::Object(mut current), Value::Object(incoming)) => {
            current.extend(incoming);
            Value::Object(current)
        }
        (_, incoming) => incoming,
    }
}

fn work_from_row(row: &SqliteRow) -> Result<DiscoveredWork> {
    let authors = json_column(row, "authors_json")?;
    let metadata = json_column(row, "metadata_json")?;
    Ok(DiscoveredWork {
        id: row.try_get("id")?,
        metadata: WorkMetadata {
            canonical_key: row.try_get("canonical_key")?,
            doi: row.try_get("doi")?,
            arxiv_id: row.try_get("arxiv_id")?,
            openalex_id: row.try_get("openalex_id")?,
            title: row.try_get("title")?,
            authors,
            year: row.try_get("year")?,
            abstract_text: row.try_get("abstract_text")?,
            source_url: row.try_get("source_url")?,
            pdf_url: row.try_get("pdf_url")?,
            evidence_level: row.try_get("evidence_level")?,
            metadata,
        },
    })
}

fn work_from_joined_row(row: &SqliteRow) -> Result<DiscoveredWork> {
    let authors = json_column(row, "authors_json")?;
    let metadata = json_column(row, "metadata_json")?;
    Ok(DiscoveredWork {
        id: row.try_get("work_id")?,
        metadata: WorkMetadata {
            canonical_key: row.try_get("canonical_key")?,
            doi: row.try_get("doi")?,
            arxiv_id: row.try_get("arxiv_id")?,
            openalex_id: row.try_get("openalex_id")?,
            title: row.try_get("title")?,
            authors,
            year: row.try_get("year")?,
            abstract_text: row.try_get("abstract_text")?,
            source_url: row.try_get("source_url")?,
            pdf_url: row.try_get("pdf_url")?,
            evidence_level: row.try_get("evidence_level")?,
            metadata,
        },
    })
}

fn search_from_row(row: &SqliteRow) -> Result<LiteratureSearchRun> {
    Ok(LiteratureSearchRun {
        id: row.try_get("id")?,
        project_id: row.try_get("project_id")?,
        conversation_id: row.try_get("conversation_id")?,
        message_id: row.try_get("message_id")?,
        trigger: row.try_get("trigger_type")?,
        question: row.try_get("question")?,
        query_plan: json_column(row, "query_plan_json")?,
        state: row.try_get("state")?,
        provider_status: json_column(row, "provider_status_json")?,
        error: row.try_get("error")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn search_result_from_row(row: &SqliteRow) -> Result<LiteratureSearchResult> {
    Ok(LiteratureSearchResult {
        search_run_id: row.try_get("search_run_id")?,
        work: work_from_joined_row(row)?,
        providers: json_column(row, "providers_json")?,
        best_rank: row.try_get("best_rank")?,
        provider_scores: json_column(row, "provider_scores_json")?,
        raw_results: json_column(row, "raw_results_json")?,
        created_at: row.try_get("created_at")?,
    })
}

fn candidate_from_row(row: &SqliteRow) -> Result<ProjectCandidate> {
    Ok(ProjectCandidate {
        project_id: row.try_get("project_id")?,
        work: work_from_joined_row(row)?,
        status: row.try_get("candidate_status")?,
        relevance_reason: row.try_get("relevance_reason")?,
        relevance_tags: json_column(row, "relevance_tags_json")?,
        evidence_level: row.try_get("candidate_evidence_level")?,
        discovered_by_search_run_id: row.try_get("discovered_by_search_run_id")?,
        discovered_by_conversation_id: row.try_get("discovered_by_conversation_id")?,
        import_task_id: row.try_get("import_task_id")?,
        paper_id: row.try_get("paper_id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn json_column<T: DeserializeOwned>(row: &SqliteRow, column: &str) -> Result<T> {
    let value: String = row.try_get(column)?;
    serde_json::from_str(&value).with_context(|| format!("parse JSON column {column}"))
}

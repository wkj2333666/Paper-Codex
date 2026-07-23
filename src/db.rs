use crate::{
    domain::{Paper, Project, Task, TaskEvent, TaskState},
    graph::{GraphEdge, GraphNode, GraphPayload, KnowledgeKind},
};
use anyhow::{bail, Context, Result};
use sqlx::Row;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

type KnowledgeEdgeRow = (String, String, String, String, i64, f64, String, String);

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProjectImpact {
    pub direct_papers: i64,
    pub descendant_projects: i64,
    pub descendant_papers: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PaperImpact {
    pub project_references: i64,
    pub graph_edges: i64,
    pub revisions: i64,
}

const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS papers (
  id TEXT PRIMARY KEY, title TEXT NOT NULL, authors_json TEXT NOT NULL DEFAULT '[]',
  year INTEGER, doi TEXT, arxiv_id TEXT, canonical_sha256 TEXT, source_url TEXT, note_path TEXT,
  deleted_at TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS revisions (
  paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE, sha256 TEXT NOT NULL,
  source_url TEXT, artifact_path TEXT NOT NULL, retrieved_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (paper_id, sha256)
);
CREATE TABLE IF NOT EXISTS projects (
  id TEXT PRIMARY KEY, slug TEXT NOT NULL UNIQUE, name TEXT NOT NULL, purpose TEXT NOT NULL DEFAULT '',
  parent_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS project_papers (
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (project_id, paper_id)
);
CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL, state TEXT NOT NULL, input_json TEXT NOT NULL,
  paper_id TEXT, project_id TEXT, thread_id TEXT, error TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS task_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT, task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  event_type TEXT NOT NULL, payload_json TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS task_events_task_id ON task_events(task_id, id);
CREATE TABLE IF NOT EXISTS relations (
  id TEXT PRIMARY KEY, source_paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  target_paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE, relation_type TEXT NOT NULL,
  evidence_json TEXT NOT NULL, hypothesis INTEGER NOT NULL DEFAULT 0,
  UNIQUE(source_paper_id, target_paper_id, relation_type)
);
CREATE TABLE IF NOT EXISTS chat_messages (
  id TEXT PRIMARY KEY, scope_type TEXT NOT NULL, scope_id TEXT, role TEXT NOT NULL,
  content TEXT NOT NULL, thread_id TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
  entity_type UNINDEXED, entity_id UNINDEXED, title, body, tokenize='unicode61 remove_diacritics 2'
);
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS paper_analyses (
  paper_id TEXT PRIMARY KEY REFERENCES papers(id) ON DELETE CASCADE,
  revision TEXT NOT NULL, analysis_json TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS knowledge_nodes (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL, label TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '', paper_id TEXT REFERENCES papers(id) ON DELETE CASCADE,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS paper_knowledge_nodes (
  paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  revision TEXT NOT NULL, node_id TEXT NOT NULL REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
  PRIMARY KEY (paper_id, node_id)
);
CREATE TABLE IF NOT EXISTS knowledge_edges (
  id TEXT PRIMARY KEY, source_id TEXT NOT NULL REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
  target_id TEXT NOT NULL REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
  relation_type TEXT NOT NULL, hypothesis INTEGER NOT NULL DEFAULT 0,
  confidence REAL NOT NULL DEFAULT 1.0, evidence_json TEXT NOT NULL DEFAULT '[]',
  origin_paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  revision TEXT NOT NULL, updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS knowledge_edges_source ON knowledge_edges(source_id);
CREATE INDEX IF NOT EXISTS knowledge_edges_target ON knowledge_edges(target_id);
CREATE INDEX IF NOT EXISTS knowledge_edges_origin ON knowledge_edges(origin_paper_id);
"#;

const CONVERSATION_BASE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS conversations (
  id TEXT PRIMARY KEY, title TEXT NOT NULL, thread_id TEXT, status TEXT NOT NULL DEFAULT 'idle',
  archived_at TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS conversation_scopes (
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  scope_type TEXT NOT NULL CHECK(scope_type IN ('global','paper','project')),
  scope_id TEXT,
  added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CHECK((scope_type='global' AND scope_id IS NULL) OR (scope_type IN ('paper','project') AND scope_id IS NOT NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS conversation_scopes_specific
  ON conversation_scopes(conversation_id,scope_type,scope_id)
  WHERE scope_type IN ('paper','project');
CREATE UNIQUE INDEX IF NOT EXISTS conversation_scopes_global
  ON conversation_scopes(conversation_id)
  WHERE scope_type='global';
"#;

const CONVERSATION_EXTENDED_SCHEMA: &str = r#"
CREATE INDEX IF NOT EXISTS chat_messages_conversation
  ON chat_messages(conversation_id,created_at);
CREATE TABLE IF NOT EXISTS message_citations (
  id TEXT PRIMARY KEY, message_id TEXT NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
  paper_id TEXT NOT NULL, revision TEXT NOT NULL, page INTEGER NOT NULL CHECK(page>0),
  section TEXT, locator TEXT, quote TEXT NOT NULL, prefix TEXT NOT NULL DEFAULT '',
  suffix TEXT NOT NULL DEFAULT '', explanation TEXT NOT NULL DEFAULT '',
  match_status TEXT NOT NULL DEFAULT 'unmatched', created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS message_citations_message ON message_citations(message_id);
CREATE INDEX IF NOT EXISTS message_citations_paper ON message_citations(paper_id,revision,page);
CREATE TABLE IF NOT EXISTS annotations (
  id TEXT PRIMARY KEY, citation_id TEXT NOT NULL REFERENCES message_citations(id) ON DELETE CASCADE,
  paper_id TEXT NOT NULL, revision TEXT NOT NULL, source_message_id TEXT NOT NULL,
  kind TEXT NOT NULL, body TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'visible',
  availability TEXT NOT NULL DEFAULT 'available', created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(citation_id)
);
CREATE INDEX IF NOT EXISTS annotations_paper ON annotations(paper_id,revision,state);
CREATE TABLE IF NOT EXISTS annotation_anchors (
  annotation_id TEXT NOT NULL REFERENCES annotations(id) ON DELETE CASCADE,
  page INTEGER NOT NULL CHECK(page>0), rect_index INTEGER NOT NULL CHECK(rect_index>=0),
  x REAL NOT NULL, y REAL NOT NULL, width REAL NOT NULL, height REAL NOT NULL,
  PRIMARY KEY(annotation_id,rect_index)
);
CREATE TABLE IF NOT EXISTS conversation_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  message_id TEXT REFERENCES chat_messages(id) ON DELETE SET NULL,
  event_type TEXT NOT NULL, payload_json TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS conversation_events_replay ON conversation_events(conversation_id,id);
"#;

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn connect(url: &str) -> Result<Self> {
        let max_connections = if url.contains(":memory:") { 1 } else { 4 };
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect(url)
            .await
            .with_context(|| format!("connect sqlite database: {url}"))?;
        sqlx::raw_sql(SCHEMA)
            .execute(&pool)
            .await
            .context("migrate database")?;
        sqlx::raw_sql(CONVERSATION_BASE_SCHEMA)
            .execute(&pool)
            .await
            .context("create conversation base schema")?;
        Self::migrate_legacy_schema(&pool).await?;
        sqlx::raw_sql(CONVERSATION_EXTENDED_SCHEMA)
            .execute(&pool)
            .await
            .context("create conversation extended schema")?;
        Ok(Self { pool })
    }

    async fn migrate_legacy_schema(pool: &SqlitePool) -> Result<()> {
        if !has_column(pool, "projects", "parent_id").await? {
            sqlx::query(
                "ALTER TABLE projects ADD COLUMN parent_id TEXT REFERENCES projects(id) ON DELETE SET NULL",
            )
            .execute(pool)
            .await?;
        }
        if !has_column(pool, "papers", "deleted_at").await? {
            sqlx::query("ALTER TABLE papers ADD COLUMN deleted_at TEXT")
                .execute(pool)
                .await?;
        }
        if !has_column(pool, "chat_messages", "conversation_id").await? {
            Self::migrate_legacy_chat_messages(pool).await?;
        }
        Ok(())
    }

    async fn migrate_legacy_chat_messages(pool: &SqlitePool) -> Result<()> {
        let rows = sqlx::query(
            "SELECT id,scope_type,scope_id,role,content,thread_id,created_at FROM chat_messages ORDER BY created_at,rowid",
        )
        .fetch_all(pool)
        .await?;
        let mut tx = pool.begin().await?;
        sqlx::query(
            r#"CREATE TABLE chat_messages_v2 (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                role TEXT NOT NULL, content TEXT NOT NULL, turn_id TEXT,
                status TEXT NOT NULL DEFAULT 'completed', error TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )"#,
        )
        .execute(&mut *tx)
        .await?;

        let mut conversations = HashMap::<String, String>::new();
        for row in rows {
            let id: String = row.try_get("id")?;
            let scope_type: String = row.try_get("scope_type")?;
            let scope_id: Option<String> = row.try_get("scope_id")?;
            let role: String = row.try_get("role")?;
            let content: String = row.try_get("content")?;
            let thread_id: Option<String> = row.try_get("thread_id")?;
            let created_at: String = row.try_get("created_at")?;
            let key = thread_id
                .as_ref()
                .map(|value| format!("thread:{value}"))
                .unwrap_or_else(|| {
                    format!("scope:{scope_type}:{}", scope_id.as_deref().unwrap_or(""))
                });
            let conversation_id = if let Some(existing) = conversations.get(&key) {
                existing.clone()
            } else {
                let conversation_id = Uuid::new_v4().to_string();
                let title = match scope_type.as_str() {
                    "paper" => "历史论文对话",
                    "project" => "历史项目对话",
                    _ => "历史全局对话",
                };
                sqlx::query("INSERT INTO conversations(id,title,thread_id,status,created_at,updated_at) VALUES(?,?,?,'idle',?,?)")
                    .bind(&conversation_id)
                    .bind(title)
                    .bind(&thread_id)
                    .bind(&created_at)
                    .bind(&created_at)
                    .execute(&mut *tx)
                    .await?;
                match (scope_type.as_str(), scope_id.as_deref()) {
                    ("paper" | "project", Some(scope_id)) if !scope_id.trim().is_empty() => {
                        sqlx::query("INSERT OR IGNORE INTO conversation_scopes(conversation_id,scope_type,scope_id,added_at) VALUES(?,?,?,?)")
                            .bind(&conversation_id)
                            .bind(&scope_type)
                            .bind(scope_id)
                            .bind(&created_at)
                            .execute(&mut *tx)
                            .await?;
                    }
                    _ => {
                        sqlx::query("INSERT OR IGNORE INTO conversation_scopes(conversation_id,scope_type,scope_id,added_at) VALUES(?,'global',NULL,?)")
                            .bind(&conversation_id)
                            .bind(&created_at)
                            .execute(&mut *tx)
                            .await?;
                    }
                }
                conversations.insert(key, conversation_id.clone());
                conversation_id
            };
            sqlx::query("INSERT INTO chat_messages_v2(id,conversation_id,role,content,status,created_at,updated_at) VALUES(?,?,?,?,'completed',?,?)")
                .bind(id)
                .bind(conversation_id)
                .bind(role)
                .bind(content)
                .bind(&created_at)
                .bind(&created_at)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("DROP TABLE chat_messages")
            .execute(&mut *tx)
            .await?;
        sqlx::query("ALTER TABLE chat_messages_v2 RENAME TO chat_messages")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn insert_paper(&self, id: &str, title: &str) -> Result<()> {
        sqlx::query("INSERT INTO papers(id,title) VALUES(?,?) ON CONFLICT(id) DO UPDATE SET title=excluded.title,updated_at=CURRENT_TIMESTAMP")
            .bind(id).bind(title).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn upsert_paper(&self, paper: &Paper) -> Result<()> {
        sqlx::query(r#"INSERT INTO papers(id,title,authors_json,year,doi,arxiv_id,canonical_sha256,source_url,note_path)
            VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET title=excluded.title,
            authors_json=excluded.authors_json,year=excluded.year,doi=excluded.doi,arxiv_id=excluded.arxiv_id,
            canonical_sha256=excluded.canonical_sha256,source_url=excluded.source_url,note_path=excluded.note_path,
            updated_at=CURRENT_TIMESTAMP"#)
            .bind(&paper.id).bind(&paper.title).bind(&paper.authors_json).bind(paper.year)
            .bind(&paper.doi).bind(&paper.arxiv_id).bind(&paper.canonical_sha256)
            .bind(&paper.source_url).bind(&paper.note_path).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn create_project(&self, slug: &str, name: &str, purpose: &str) -> Result<String> {
        self.create_project_with_parent(slug, name, purpose, None)
            .await
    }

    pub async fn create_project_with_parent(
        &self,
        slug: &str,
        name: &str,
        purpose: &str,
        parent_id: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO projects(id,slug,name,purpose,parent_id) VALUES(?,?,?,?,?)")
            .bind(&id)
            .bind(slug)
            .bind(name)
            .bind(purpose)
            .bind(parent_id)
            .execute(&self.pool)
            .await?;
        Ok(id)
    }

    pub async fn update_project(
        &self,
        id: &str,
        name: &str,
        purpose: &str,
        parent_id: Option<&str>,
    ) -> Result<()> {
        if parent_id == Some(id) {
            anyhow::bail!("project cannot be its own parent");
        }
        if let Some(parent_id) = parent_id {
            if self.get_project(parent_id).await?.is_none() {
                anyhow::bail!("parent project does not exist");
            }
            let descendants: i64 = sqlx::query_scalar(
                r#"WITH RECURSIVE descendants(id) AS (
                    SELECT id FROM projects WHERE parent_id=?
                    UNION ALL
                    SELECT p.id FROM projects p JOIN descendants d ON p.parent_id=d.id
                ) SELECT COUNT(*) FROM descendants WHERE id=?"#,
            )
            .bind(id)
            .bind(parent_id)
            .fetch_one(&self.pool)
            .await?;
            if descendants > 0 {
                anyhow::bail!("moving project would create a cycle");
            }
        }
        let changed = sqlx::query("UPDATE projects SET name=?,purpose=?,parent_id=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
            .bind(name.trim())
            .bind(purpose.trim())
            .bind(parent_id)
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if changed == 0 {
            anyhow::bail!("project does not exist");
        }
        Ok(())
    }

    pub async fn project_impact(&self, id: &str) -> Result<ProjectImpact> {
        let direct_papers = sqlx::query_scalar(
            "SELECT COUNT(*) FROM project_papers pp JOIN papers p ON p.id=pp.paper_id WHERE pp.project_id=? AND p.deleted_at IS NULL",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        let (descendant_projects, descendant_papers): (i64, i64) = sqlx::query_as(
            r#"WITH RECURSIVE descendants(id) AS (
                SELECT id FROM projects WHERE parent_id=?
                UNION ALL
                SELECT p.id FROM projects p JOIN descendants d ON p.parent_id=d.id
            ) SELECT
                (SELECT COUNT(*) FROM descendants),
                (SELECT COUNT(DISTINCT pp.paper_id) FROM project_papers pp
                 JOIN descendants d ON d.id=pp.project_id
                 JOIN papers p ON p.id=pp.paper_id WHERE p.deleted_at IS NULL)"#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        Ok(ProjectImpact {
            direct_papers,
            descendant_projects,
            descendant_papers,
        })
    }

    pub async fn delete_project(&self, id: &str, subtree: bool) -> Result<()> {
        let project = self
            .get_project(id)
            .await?
            .context("project does not exist")?;
        let mut tx = self.pool.begin().await?;
        if subtree {
            sqlx::query(
                r#"WITH RECURSIVE descendants(id) AS (
                    SELECT ?
                    UNION ALL
                    SELECT p.id FROM projects p JOIN descendants d ON p.parent_id=d.id
                ) DELETE FROM projects WHERE id IN (SELECT id FROM descendants)"#,
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query("UPDATE projects SET parent_id=? WHERE parent_id=?")
                .bind(project.parent_id)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM projects WHERE id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn add_paper_to_project(&self, paper_id: &str, project_id: &str) -> Result<()> {
        sqlx::query("INSERT OR IGNORE INTO project_papers(project_id,paper_id) VALUES(?,?)")
            .bind(project_id)
            .bind(paper_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn remove_paper_from_project(&self, paper_id: &str, project_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM project_papers WHERE project_id=? AND paper_id=?")
            .bind(project_id)
            .bind(paper_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn paper_project_ids(&self, paper_id: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT project_id FROM project_papers WHERE paper_id=? ORDER BY project_id",
        )
        .bind(paper_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn paper_count(&self) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM papers WHERE deleted_at IS NULL")
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn list_papers(&self) -> Result<Vec<Paper>> {
        Ok(
            sqlx::query_as(
                "SELECT * FROM papers WHERE deleted_at IS NULL ORDER BY updated_at DESC",
            )
            .fetch_all(&self.pool)
            .await?,
        )
    }

    pub async fn get_paper(&self, id: &str) -> Result<Option<Paper>> {
        Ok(sqlx::query_as("SELECT * FROM papers WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        Ok(sqlx::query_as("SELECT * FROM projects ORDER BY name")
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn list_trashed_papers(&self) -> Result<Vec<Paper>> {
        Ok(sqlx::query_as(
            "SELECT * FROM papers WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn papers_without_graph(&self) -> Result<Vec<Paper>> {
        Ok(sqlx::query_as(
            r#"SELECT p.* FROM papers p
               WHERE p.deleted_at IS NULL
                 AND NOT EXISTS (SELECT 1 FROM paper_knowledge_nodes pkn WHERE pkn.paper_id=p.id)
               ORDER BY p.updated_at DESC"#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn trash_paper(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE papers SET deleted_at=CURRENT_TIMESTAMP,updated_at=CURRENT_TIMESTAMP WHERE id=?",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn restore_paper(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE papers SET deleted_at=NULL,updated_at=CURRENT_TIMESTAMP WHERE id=?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn paper_impact(&self, id: &str) -> Result<PaperImpact> {
        let project_references =
            sqlx::query_scalar("SELECT COUNT(*) FROM project_papers WHERE paper_id=?")
                .bind(id)
                .fetch_one(&self.pool)
                .await?;
        let relation_edges: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM relations WHERE source_paper_id=? OR target_paper_id=?",
        )
        .bind(id)
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        let knowledge_edges: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM knowledge_edges WHERE origin_paper_id=? OR source_id=? OR target_id=?",
        )
        .bind(id)
        .bind(id)
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        let revisions = sqlx::query_scalar("SELECT COUNT(*) FROM revisions WHERE paper_id=?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        Ok(PaperImpact {
            project_references,
            graph_edges: relation_edges + knowledge_edges,
            revisions,
        })
    }

    pub async fn permanently_delete_paper(&self, id: &str) -> Result<()> {
        let deleted_at: Option<(Option<String>,)> =
            sqlx::query_as("SELECT deleted_at FROM papers WHERE id=?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        match deleted_at {
            None => anyhow::bail!("paper does not exist"),
            Some((None,)) => anyhow::bail!("paper must be in trash before permanent deletion"),
            Some((Some(_),)) => {}
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE annotations SET availability='paper-missing',updated_at=CURRENT_TIMESTAMP WHERE paper_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM knowledge_fts WHERE entity_type='paper' AND entity_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM papers WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_paper_analysis(
        &self,
        paper_id: &str,
        revision: &str,
        analysis: &serde_json::Value,
    ) -> Result<()> {
        sqlx::query("INSERT INTO paper_analyses(paper_id,revision,analysis_json) VALUES(?,?,?) ON CONFLICT(paper_id) DO UPDATE SET revision=excluded.revision,analysis_json=excluded.analysis_json,updated_at=CURRENT_TIMESTAMP")
            .bind(paper_id)
            .bind(revision)
            .bind(serde_json::to_string(analysis)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn paper_analysis(&self, paper_id: &str) -> Result<Option<serde_json::Value>> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT analysis_json FROM paper_analyses WHERE paper_id=?")
                .bind(paper_id)
                .fetch_optional(&self.pool)
                .await?;
        value
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }

    pub async fn replace_paper_graph(
        &self,
        paper_id: &str,
        revision: &str,
        nodes: &[GraphNode],
        edges: &[GraphEdge],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM knowledge_edges WHERE origin_paper_id=?")
            .bind(paper_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM paper_knowledge_nodes WHERE paper_id=?")
            .bind(paper_id)
            .execute(&mut *tx)
            .await?;
        for node in nodes {
            sqlx::query("INSERT INTO knowledge_nodes(id,kind,label,description,paper_id) VALUES(?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET kind=excluded.kind,label=excluded.label,description=excluded.description,paper_id=COALESCE(excluded.paper_id,knowledge_nodes.paper_id),updated_at=CURRENT_TIMESTAMP")
                .bind(&node.id)
                .bind(node.kind.as_str())
                .bind(&node.label)
                .bind(&node.description)
                .bind(&node.paper_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO paper_knowledge_nodes(paper_id,revision,node_id) VALUES(?,?,?)",
            )
            .bind(paper_id)
            .bind(revision)
            .bind(&node.id)
            .execute(&mut *tx)
            .await?;
        }
        for edge in edges {
            sqlx::query("INSERT INTO knowledge_edges(id,source_id,target_id,relation_type,hypothesis,confidence,evidence_json,origin_paper_id,revision) VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(&edge.id)
                .bind(&edge.source)
                .bind(&edge.target)
                .bind(&edge.relation_type)
                .bind(edge.hypothesis)
                .bind(edge.confidence)
                .bind(serde_json::to_string(&edge.evidence)?)
                .bind(paper_id)
                .bind(revision)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn graph_for_paper(&self, paper_id: &str) -> Result<GraphPayload> {
        let node_rows: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT n.id,n.kind,n.label,n.description,n.paper_id FROM knowledge_nodes n JOIN paper_knowledge_nodes pkn ON pkn.node_id=n.id WHERE pkn.paper_id=? ORDER BY n.kind,n.label",
        )
        .bind(paper_id)
        .fetch_all(&self.pool)
        .await?;
        let nodes = node_rows
            .into_iter()
            .map(|(id, kind, label, description, paper_id)| GraphNode {
                id,
                kind: parse_kind(&kind),
                label,
                description,
                paper_id,
            })
            .collect::<Vec<_>>();
        let edge_rows: Vec<(String, String, String, String, i64, f64, String)> = sqlx::query_as(
            "SELECT id,source_id,target_id,relation_type,hypothesis,confidence,evidence_json FROM knowledge_edges WHERE origin_paper_id=? ORDER BY id",
        )
        .bind(paper_id)
        .fetch_all(&self.pool)
        .await?;
        let edges = edge_rows
            .into_iter()
            .map(
                |(id, source, target, relation_type, hypothesis, confidence, evidence)| {
                    Ok(GraphEdge {
                        id,
                        source,
                        target,
                        relation_type,
                        hypothesis: hypothesis != 0,
                        confidence,
                        evidence: serde_json::from_str(&evidence)?,
                    })
                },
            )
            .collect::<Result<Vec<_>>>()?;
        Ok(GraphPayload { nodes, edges })
    }

    pub async fn query_graph(
        &self,
        project_id: Option<&str>,
        paper_id: Option<&str>,
        kinds: &[String],
        include_hypotheses: bool,
    ) -> Result<GraphPayload> {
        let paper_ids: Vec<String> = if let Some(paper_id) = paper_id {
            sqlx::query_scalar("SELECT id FROM papers WHERE id=? AND deleted_at IS NULL")
                .bind(paper_id)
                .fetch_all(&self.pool)
                .await?
        } else if let Some(project_id) = project_id {
            sqlx::query_scalar(
                r#"WITH RECURSIVE scope(id) AS (
                    SELECT ?
                    UNION ALL
                    SELECT p.id FROM projects p JOIN scope s ON p.parent_id=s.id
                ) SELECT DISTINCT pp.paper_id FROM project_papers pp
                  JOIN scope s ON s.id=pp.project_id
                  JOIN papers p ON p.id=pp.paper_id
                  WHERE p.deleted_at IS NULL"#,
            )
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_scalar("SELECT id FROM papers WHERE deleted_at IS NULL")
                .fetch_all(&self.pool)
                .await?
        };
        if paper_ids.is_empty() {
            return Ok(GraphPayload::default());
        }
        let paper_ids = paper_ids.into_iter().collect::<HashSet<_>>();
        let rows: Vec<(String, String, String, String, Option<String>, String)> = sqlx::query_as(
            r#"SELECT n.id,n.kind,n.label,n.description,n.paper_id,pkn.paper_id
               FROM knowledge_nodes n
               JOIN paper_knowledge_nodes pkn ON pkn.node_id=n.id
               JOIN papers p ON p.id=pkn.paper_id
               WHERE p.deleted_at IS NULL
               ORDER BY n.kind,n.label"#,
        )
        .fetch_all(&self.pool)
        .await?;
        let mut node_map = HashMap::new();
        for (id, kind, label, description, owning_paper, linked_paper) in rows {
            if !paper_ids.contains(&linked_paper) {
                continue;
            }
            let kind = parse_kind(&kind);
            if !kinds.is_empty() && !kinds.iter().any(|value| value == kind.as_str()) {
                continue;
            }
            node_map.entry(id.clone()).or_insert(GraphNode {
                id,
                kind,
                label,
                description,
                paper_id: owning_paper,
            });
        }
        let node_ids = node_map.keys().cloned().collect::<HashSet<_>>();
        let edge_rows: Vec<KnowledgeEdgeRow> = sqlx::query_as(
                "SELECT id,source_id,target_id,relation_type,hypothesis,confidence,evidence_json,origin_paper_id FROM knowledge_edges ORDER BY id",
            )
            .fetch_all(&self.pool)
            .await?;
        let mut edges = Vec::new();
        for (id, source, target, relation_type, hypothesis, confidence, evidence, origin) in
            edge_rows
        {
            if !paper_ids.contains(&origin)
                || !node_ids.contains(&source)
                || !node_ids.contains(&target)
                || (!include_hypotheses && hypothesis != 0)
            {
                continue;
            }
            edges.push(GraphEdge {
                id,
                source,
                target,
                relation_type,
                hypothesis: hypothesis != 0,
                confidence,
                evidence: serde_json::from_str(&evidence)?,
            });
        }
        let mut nodes = node_map.into_values().collect::<Vec<_>>();
        nodes.sort_by(|left, right| {
            left.kind
                .as_str()
                .cmp(right.kind.as_str())
                .then_with(|| left.label.cmp(&right.label))
        });
        Ok(GraphPayload { nodes, edges })
    }

    pub async fn get_task(&self, id: &str) -> Result<Option<Task>> {
        Ok(sqlx::query_as("SELECT * FROM tasks WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn create_task(&self, kind: &str, input_json: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO tasks(id,kind,state,input_json) VALUES(?,?,?,?)")
            .bind(&id)
            .bind(kind)
            .bind(TaskState::Queued.as_str())
            .bind(input_json)
            .execute(&self.pool)
            .await?;
        Ok(id)
    }

    pub async fn transition_task(
        &self,
        id: &str,
        next: TaskState,
        error: Option<&str>,
    ) -> Result<()> {
        let current: String = sqlx::query_scalar("SELECT state FROM tasks WHERE id=?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        let current: TaskState = current.parse().map_err(anyhow::Error::msg)?;
        if !current.can_transition_to(next) {
            anyhow::bail!("illegal task transition {current} -> {next}");
        }
        sqlx::query("UPDATE tasks SET state=?,error=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
            .bind(next.as_str())
            .bind(error)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn force_task_state(
        &self,
        id: &str,
        state: TaskState,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE tasks SET state=?,error=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
            .bind(state.as_str())
            .bind(error)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_task_context(
        &self,
        id: &str,
        paper_id: Option<&str>,
        project_id: Option<&str>,
        thread_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE tasks SET paper_id=COALESCE(?,paper_id),project_id=COALESCE(?,project_id),thread_id=COALESCE(?,thread_id),updated_at=CURRENT_TIMESTAMP WHERE id=?")
            .bind(paper_id).bind(project_id).bind(thread_id).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn append_event(
        &self,
        task_id: &str,
        event_type: &str,
        payload_json: &str,
    ) -> Result<TaskEvent> {
        let result =
            sqlx::query("INSERT INTO task_events(task_id,event_type,payload_json) VALUES(?,?,?)")
                .bind(task_id)
                .bind(event_type)
                .bind(payload_json)
                .execute(&self.pool)
                .await?;
        let id = result.last_insert_rowid();
        Ok(sqlx::query_as("SELECT * FROM task_events WHERE id=?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?)
    }

    pub async fn events_after(&self, after: i64) -> Result<Vec<TaskEvent>> {
        Ok(
            sqlx::query_as("SELECT * FROM task_events WHERE id>? ORDER BY id LIMIT 1000")
                .bind(after)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn list_tasks(&self, limit: i64) -> Result<Vec<Task>> {
        Ok(
            sqlx::query_as("SELECT * FROM tasks ORDER BY created_at DESC LIMIT ?")
                .bind(limit)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn dismiss_task(&self, id: &str) -> Result<bool> {
        let mut transaction = self.pool.begin().await?;
        let state = sqlx::query_scalar::<_, String>("SELECT state FROM tasks WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?;
        let Some(state) = state else {
            transaction.rollback().await?;
            return Ok(false);
        };
        if !matches!(state.as_str(), "done" | "failed" | "cancelled") {
            bail!("only terminal tasks can be dismissed");
        }
        let deleted = sqlx::query("DELETE FROM tasks WHERE id=?")
            .bind(id)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        transaction.commit().await?;
        Ok(deleted == 1)
    }

    pub async fn resumable_task_ids(&self) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT id FROM tasks WHERE state NOT IN ('done','cancelled','failed','needs-input') ORDER BY created_at")
            .fetch_all(&self.pool).await?)
    }

    pub async fn reset_task_for_resume(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE tasks SET state='queued',error=NULL,updated_at=CURRENT_TIMESTAMP WHERE id=?",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_project(&self, id: &str) -> Result<Option<Project>> {
        Ok(sqlx::query_as("SELECT * FROM projects WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn project_paper_ids(&self, id: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT pp.paper_id FROM project_papers pp JOIN papers p ON p.id=pp.paper_id WHERE pp.project_id=? AND p.deleted_at IS NULL ORDER BY pp.added_at",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn add_revision(
        &self,
        paper_id: &str,
        sha256: &str,
        source_url: Option<&str>,
        artifact_path: &str,
    ) -> Result<()> {
        sqlx::query("INSERT OR IGNORE INTO revisions(paper_id,sha256,source_url,artifact_path) VALUES(?,?,?,?)")
            .bind(paper_id).bind(sha256).bind(source_url).bind(artifact_path).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn upsert_relation(
        &self,
        source: &str,
        target: &str,
        relation_type: &str,
        evidence_json: &str,
        hypothesis: bool,
    ) -> Result<()> {
        let id = format!("{}:{}:{}", source, relation_type, target);
        sqlx::query("INSERT INTO relations(id,source_paper_id,target_paper_id,relation_type,evidence_json,hypothesis) VALUES(?,?,?,?,?,?) ON CONFLICT(source_paper_id,target_paper_id,relation_type) DO UPDATE SET evidence_json=excluded.evidence_json,hypothesis=excluded.hypothesis")
            .bind(id).bind(source).bind(target).bind(relation_type).bind(evidence_json).bind(hypothesis).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn relations_for(&self, paper_id: &str) -> Result<Vec<serde_json::Value>> {
        let rows: Vec<(String,String,String,i64)> = sqlx::query_as("SELECT source_paper_id,target_paper_id,relation_type,hypothesis FROM relations WHERE source_paper_id=? OR target_paper_id=?")
            .bind(paper_id).bind(paper_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|(source,target,kind,hypothesis)| serde_json::json!({"source":source,"target":target,"type":kind,"hypothesis":hypothesis!=0})).collect())
    }
}

async fn has_column(pool: &SqlitePool, table: &str, column: &str) -> Result<bool> {
    let sql = format!("PRAGMA table_info({table})");
    let names = sqlx::query(&sql)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.try_get::<String, _>("name"))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(names.iter().any(|name| name == column))
}

fn parse_kind(value: &str) -> KnowledgeKind {
    match value {
        "concept" => KnowledgeKind::Concept,
        "method" => KnowledgeKind::Method,
        "dataset" => KnowledgeKind::Dataset,
        "finding" => KnowledgeKind::Finding,
        _ => KnowledgeKind::Paper,
    }
}

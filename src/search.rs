use crate::db::Database;
use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SearchResult {
    pub entity_type: String,
    pub entity_id: String,
    pub title: String,
    pub snippet: String,
}

#[derive(Clone)]
pub struct SearchIndex {
    db: Database,
}

impl SearchIndex {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn upsert(
        &self,
        entity_type: &str,
        entity_id: &str,
        title: &str,
        body: &str,
    ) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;
        sqlx::query("DELETE FROM knowledge_fts WHERE entity_type=? AND entity_id=?")
            .bind(entity_type)
            .bind(entity_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO knowledge_fts(entity_type,entity_id,title,body) VALUES(?,?,?,?)")
            .bind(entity_type)
            .bind(entity_id)
            .bind(title)
            .bind(body)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn query(&self, query: &str, entity_type: Option<&str>) -> Result<Vec<SearchResult>> {
        if query.trim().is_empty() {
            return Ok(vec![]);
        }
        let rows = if let Some(kind) = entity_type {
            sqlx::query_as("SELECT entity_type,entity_id,title,snippet(knowledge_fts,3,'<mark>','</mark>',' … ',24) AS snippet FROM knowledge_fts WHERE knowledge_fts MATCH ? AND entity_type=? ORDER BY bm25(knowledge_fts) LIMIT 50")
                .bind(query).bind(kind).fetch_all(self.db.pool()).await?
        } else {
            sqlx::query_as("SELECT entity_type,entity_id,title,snippet(knowledge_fts,3,'<mark>','</mark>',' … ',24) AS snippet FROM knowledge_fts WHERE knowledge_fts MATCH ? ORDER BY bm25(knowledge_fts) LIMIT 50")
                .bind(query).fetch_all(self.db.pool()).await?
        };
        Ok(rows)
    }
}

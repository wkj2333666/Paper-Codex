use crate::codex::CodexRunSettings;
use crate::db::Database;
use crate::prompts::ConversationAnswer;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub thread_id: Option<String>,
    pub status: String,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct ConversationScope {
    pub conversation_id: String,
    pub scope_type: String,
    pub scope_id: Option<String>,
    pub added_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationScopeInput {
    pub scope_type: String,
    pub scope_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct ChatMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub turn_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct MessageCitation {
    pub id: String,
    pub message_id: String,
    pub paper_id: String,
    pub revision: String,
    pub page: i64,
    pub section: Option<String>,
    pub locator: Option<String>,
    pub quote: String,
    pub prefix: String,
    pub suffix: String,
    pub explanation: String,
    pub match_status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Annotation {
    pub id: String,
    pub citation_id: String,
    pub paper_id: String,
    pub revision: String,
    pub source_message_id: String,
    pub kind: String,
    pub body: String,
    pub state: String,
    pub availability: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct AnnotationAnchor {
    pub annotation_id: String,
    pub page: i64,
    pub rect_index: i64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperAnnotation {
    pub annotation: Annotation,
    pub citation: MessageCitation,
    pub anchors: Vec<AnnotationAnchor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationEvent {
    pub id: i64,
    pub conversation_id: String,
    pub message_id: Option<String>,
    pub event_type: String,
    pub payload: Value,
    pub created_at: String,
}

impl Database {
    pub async fn create_conversation(&self, title: &str) -> Result<Conversation> {
        let title = title.trim();
        if title.is_empty() {
            bail!("conversation title cannot be empty");
        }
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO conversations(id,title,status) VALUES(?,?,'idle')")
            .bind(&id)
            .bind(title)
            .execute(self.pool())
            .await?;
        self.get_conversation(&id)
            .await?
            .context("created conversation is missing")
    }

    pub async fn list_conversations(
        &self,
        archived: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Conversation>> {
        let archived_filter = if archived { "IS NOT NULL" } else { "IS NULL" };
        let query = format!(
            "SELECT id,title,thread_id,status,model,reasoning_effort,service_tier,archived_at,created_at,updated_at FROM conversations WHERE archived_at {archived_filter} ORDER BY updated_at DESC,rowid DESC LIMIT ? OFFSET ?"
        );
        Ok(sqlx::query_as(&query)
            .bind(limit.clamp(1, 100))
            .bind(offset.max(0))
            .fetch_all(self.pool())
            .await?)
    }

    pub async fn get_conversation(&self, id: &str) -> Result<Option<Conversation>> {
        Ok(sqlx::query_as("SELECT id,title,thread_id,status,model,reasoning_effort,service_tier,archived_at,created_at,updated_at FROM conversations WHERE id=?")
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }

    pub async fn update_conversation(
        &self,
        id: &str,
        title: Option<&str>,
        archived: Option<bool>,
    ) -> Result<Option<Conversation>> {
        if let Some(title) = title {
            let title = title.trim();
            if title.is_empty() {
                bail!("conversation title cannot be empty");
            }
            sqlx::query("UPDATE conversations SET title=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
                .bind(title)
                .bind(id)
                .execute(self.pool())
                .await?;
        }
        if let Some(archived) = archived {
            let sql = if archived {
                "UPDATE conversations SET archived_at=COALESCE(archived_at,CURRENT_TIMESTAMP),updated_at=CURRENT_TIMESTAMP WHERE id=?"
            } else {
                "UPDATE conversations SET archived_at=NULL,updated_at=CURRENT_TIMESTAMP WHERE id=?"
            };
            sqlx::query(sql).bind(id).execute(self.pool()).await?;
        }
        self.get_conversation(id).await
    }

    pub async fn update_conversation_settings(
        &self,
        id: &str,
        settings: &CodexRunSettings,
    ) -> Result<Option<Conversation>> {
        let changed = sqlx::query("UPDATE conversations SET model=?,reasoning_effort=?,service_tier=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
            .bind(&settings.model)
            .bind(&settings.reasoning_effort)
            .bind(&settings.service_tier)
            .bind(id)
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            return Ok(None);
        }
        self.get_conversation(id).await
    }

    pub async fn replace_conversation_scopes(
        &self,
        id: &str,
        scopes: &[ConversationScopeInput],
    ) -> Result<()> {
        if self.get_conversation(id).await?.is_none() {
            bail!("conversation does not exist");
        }
        let mut unique = HashSet::<(String, Option<String>)>::new();
        for scope in scopes {
            match scope.scope_type.as_str() {
                "global" if scope.scope_id.is_none() => {}
                "paper" | "project"
                    if scope
                        .scope_id
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty()) => {}
                _ => bail!("invalid conversation scope"),
            }
            unique.insert((scope.scope_type.clone(), scope.scope_id.clone()));
        }
        if unique.iter().any(|(kind, _)| kind == "global") && unique.len() > 1 {
            bail!("global scope cannot be combined with paper or project scopes");
        }

        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM conversation_scopes WHERE conversation_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let mut scopes = unique.into_iter().collect::<Vec<_>>();
        scopes.sort();
        for (scope_type, scope_id) in scopes {
            sqlx::query("INSERT INTO conversation_scopes(conversation_id,scope_type,scope_id) VALUES(?,?,?)")
                .bind(id)
                .bind(scope_type)
                .bind(scope_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn conversation_scopes(&self, id: &str) -> Result<Vec<ConversationScope>> {
        Ok(sqlx::query_as("SELECT conversation_id,scope_type,scope_id,added_at FROM conversation_scopes WHERE conversation_id=? ORDER BY scope_type,scope_id")
            .bind(id)
            .fetch_all(self.pool())
            .await?)
    }

    pub async fn append_chat_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        status: &str,
    ) -> Result<ChatMessage> {
        if !matches!(role, "user" | "assistant" | "system") {
            bail!("invalid chat message role");
        }
        if !valid_message_status(status) {
            bail!("invalid chat message status");
        }
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO chat_messages(id,conversation_id,role,content,status) VALUES(?,?,?,?,?)",
        )
        .bind(&id)
        .bind(conversation_id)
        .bind(role)
        .bind(content)
        .bind(status)
        .execute(self.pool())
        .await?;
        Ok(sqlx::query_as("SELECT id,conversation_id,role,content,turn_id,status,error,created_at,updated_at FROM chat_messages WHERE id=?")
            .bind(id)
            .fetch_one(self.pool())
            .await?)
    }

    pub async fn set_message_result(
        &self,
        id: &str,
        content: &str,
        turn_id: Option<&str>,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        if !valid_message_status(status) {
            bail!("invalid chat message status");
        }
        let changed = sqlx::query("UPDATE chat_messages SET content=?,turn_id=?,status=?,error=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
            .bind(content)
            .bind(turn_id)
            .bind(status)
            .bind(error)
            .bind(id)
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            bail!("chat message does not exist");
        }
        Ok(())
    }

    pub async fn conversation_messages(
        &self,
        conversation_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ChatMessage>> {
        Ok(sqlx::query_as("SELECT id,conversation_id,role,content,turn_id,status,error,created_at,updated_at FROM chat_messages WHERE conversation_id=? ORDER BY created_at,rowid LIMIT ? OFFSET ?")
            .bind(conversation_id)
            .bind(limit.clamp(1, 500))
            .bind(offset.max(0))
            .fetch_all(self.pool())
            .await?)
    }

    pub async fn list_conversation_messages(&self) -> Result<Vec<ChatMessage>> {
        Ok(sqlx::query_as("SELECT id,conversation_id,role,content,turn_id,status,error,created_at,updated_at FROM chat_messages ORDER BY created_at,rowid")
            .fetch_all(self.pool())
            .await?)
    }

    pub async fn get_chat_message(&self, id: &str) -> Result<Option<ChatMessage>> {
        Ok(sqlx::query_as("SELECT id,conversation_id,role,content,turn_id,status,error,created_at,updated_at FROM chat_messages WHERE id=?")
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }

    pub async fn message_status(&self, id: &str) -> Result<String> {
        sqlx::query_scalar("SELECT status FROM chat_messages WHERE id=?")
            .bind(id)
            .fetch_optional(self.pool())
            .await?
            .context("chat message does not exist")
    }

    pub async fn conversation_has_pending_turn(&self, conversation_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages WHERE conversation_id=? AND role='assistant' AND status IN ('queued','running','streaming')")
            .bind(conversation_id)
            .fetch_one(self.pool())
            .await?;
        Ok(count > 0)
    }

    pub async fn recover_conversation_message_states(&self) -> Result<()> {
        sqlx::query("UPDATE chat_messages SET status='interrupted',error='服务重启中断了回答',updated_at=CURRENT_TIMESTAMP WHERE status IN ('running','streaming')")
            .execute(self.pool())
            .await?;
        sqlx::query("UPDATE conversations SET status='idle',updated_at=CURRENT_TIMESTAMP WHERE status='running'")
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn queued_assistant_messages(&self) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT id FROM chat_messages WHERE role='assistant' AND status='queued' ORDER BY created_at,rowid")
            .fetch_all(self.pool())
            .await?)
    }

    pub async fn previous_user_message(&self, assistant_id: &str) -> Result<Option<ChatMessage>> {
        Ok(sqlx::query_as(
            r#"SELECT id,conversation_id,role,content,turn_id,status,error,created_at,updated_at
               FROM chat_messages
               WHERE role='user'
                 AND conversation_id=(SELECT conversation_id FROM chat_messages WHERE id=?1)
                 AND rowid < (SELECT rowid FROM chat_messages WHERE id=?1)
               ORDER BY rowid DESC LIMIT 1"#,
        )
        .bind(assistant_id)
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn set_conversation_runtime(
        &self,
        id: &str,
        thread_id: Option<&str>,
        status: &str,
    ) -> Result<()> {
        let changed = sqlx::query("UPDATE conversations SET thread_id=COALESCE(?,thread_id),status=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
            .bind(thread_id)
            .bind(status)
            .bind(id)
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            bail!("conversation does not exist");
        }
        Ok(())
    }

    pub async fn persist_conversation_answer(
        &self,
        message_id: &str,
        answer: &ConversationAnswer,
    ) -> Result<Vec<MessageCitation>> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM message_citations WHERE message_id=?")
            .bind(message_id)
            .execute(&mut *tx)
            .await?;
        let mut citation_ids = std::collections::HashMap::new();
        for citation in &answer.citations {
            let id = format!("{message_id}:{}", citation.id);
            sqlx::query("INSERT INTO message_citations(id,message_id,paper_id,revision,page,section,locator,quote,prefix,suffix,explanation,match_status) VALUES(?,?,?,?,?,?,?,?,?,?,?,'unmatched')")
                .bind(&id)
                .bind(message_id)
                .bind(&citation.paper_id)
                .bind(&citation.revision)
                .bind(citation.page as i64)
                .bind(&citation.section)
                .bind(&citation.locator)
                .bind(&citation.quote)
                .bind(&citation.prefix)
                .bind(&citation.suffix)
                .bind(&citation.explanation)
                .execute(&mut *tx)
                .await?;
            citation_ids.insert(citation.id.as_str(), id);
        }
        for intent in answer
            .annotation_intents
            .iter()
            .filter(|intent| intent.persist)
        {
            let citation_id = citation_ids
                .get(intent.citation_id.as_str())
                .context("annotation references missing citation")?;
            let citation = answer
                .citations
                .iter()
                .find(|citation| citation.id == intent.citation_id)
                .context("annotation citation payload is missing")?;
            sqlx::query("INSERT INTO annotations(id,citation_id,paper_id,revision,source_message_id,kind,body,state,availability) VALUES(?,?,?,?,?,?,?,'visible','available')")
                .bind(Uuid::new_v4().to_string())
                .bind(citation_id)
                .bind(&citation.paper_id)
                .bind(&citation.revision)
                .bind(message_id)
                .bind(&intent.kind)
                .bind(&intent.body)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.message_citations(message_id).await
    }

    pub async fn message_citations(&self, message_id: &str) -> Result<Vec<MessageCitation>> {
        Ok(sqlx::query_as("SELECT id,message_id,paper_id,revision,page,section,locator,quote,prefix,suffix,explanation,match_status,created_at FROM message_citations WHERE message_id=? ORDER BY rowid")
            .bind(message_id)
            .fetch_all(self.pool())
            .await?)
    }

    pub async fn pin_citation(&self, citation_id: &str) -> Result<Option<Annotation>> {
        let citation: Option<MessageCitation> = sqlx::query_as("SELECT id,message_id,paper_id,revision,page,section,locator,quote,prefix,suffix,explanation,match_status,created_at FROM message_citations WHERE id=?")
            .bind(citation_id)
            .fetch_optional(self.pool())
            .await?;
        let Some(citation) = citation else {
            return Ok(None);
        };
        let availability: String = sqlx::query_scalar(
            "SELECT CASE WHEN canonical_sha256=? THEN 'available' ELSE 'revision-stale' END FROM papers WHERE id=?",
        )
        .bind(&citation.revision)
        .bind(&citation.paper_id)
        .fetch_optional(self.pool())
        .await?
        .unwrap_or_else(|| "paper-missing".into());
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO annotations(id,citation_id,paper_id,revision,source_message_id,kind,body,state,availability) VALUES(?,?,?,?,?,'note',?,'visible',?) ON CONFLICT(citation_id) DO UPDATE SET state='visible',availability=excluded.availability,updated_at=CURRENT_TIMESTAMP")
            .bind(id)
            .bind(&citation.id)
            .bind(&citation.paper_id)
            .bind(&citation.revision)
            .bind(&citation.message_id)
            .bind(if citation.explanation.trim().is_empty() { &citation.quote } else { &citation.explanation })
            .bind(availability)
            .execute(self.pool())
            .await?;
        Ok(sqlx::query_as("SELECT id,citation_id,paper_id,revision,source_message_id,kind,body,state,availability,created_at,updated_at FROM annotations WHERE citation_id=?")
            .bind(citation_id)
            .fetch_optional(self.pool())
            .await?)
    }

    pub async fn set_annotation_state(&self, id: &str, state: &str) -> Result<Option<Annotation>> {
        if !matches!(state, "visible" | "hidden") {
            bail!("invalid annotation state");
        }
        sqlx::query("UPDATE annotations SET state=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
            .bind(state)
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(sqlx::query_as("SELECT id,citation_id,paper_id,revision,source_message_id,kind,body,state,availability,created_at,updated_at FROM annotations WHERE id=?")
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }

    pub async fn annotation_anchors(&self, annotation_id: &str) -> Result<Vec<AnnotationAnchor>> {
        Ok(sqlx::query_as("SELECT annotation_id,page,rect_index,x,y,width,height FROM annotation_anchors WHERE annotation_id=? ORDER BY rect_index")
            .bind(annotation_id)
            .fetch_all(self.pool())
            .await?)
    }

    pub async fn replace_annotation_anchors(
        &self,
        annotation_id: &str,
        anchors: &[AnnotationAnchor],
    ) -> Result<()> {
        if anchors.iter().any(|anchor| {
            anchor.annotation_id != annotation_id
                || anchor.page <= 0
                || anchor.rect_index < 0
                || ![anchor.x, anchor.y, anchor.width, anchor.height]
                    .into_iter()
                    .all(f64::is_finite)
                || anchor.x < 0.0
                || anchor.y < 0.0
                || anchor.width <= 0.0
                || anchor.height <= 0.0
                || anchor.x + anchor.width > 1.001
                || anchor.y + anchor.height > 1.001
        }) {
            bail!("invalid annotation anchor");
        }
        let mut tx = self.pool().begin().await?;
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM annotations WHERE id=?")
            .bind(annotation_id)
            .fetch_one(&mut *tx)
            .await?;
        if exists == 0 {
            bail!("annotation does not exist")
        }
        sqlx::query("DELETE FROM annotation_anchors WHERE annotation_id=?")
            .bind(annotation_id)
            .execute(&mut *tx)
            .await?;
        for anchor in anchors {
            sqlx::query("INSERT INTO annotation_anchors(annotation_id,page,rect_index,x,y,width,height) VALUES(?,?,?,?,?,?,?)")
                .bind(annotation_id)
                .bind(anchor.page)
                .bind(anchor.rect_index)
                .bind(anchor.x)
                .bind(anchor.y)
                .bind(anchor.width)
                .bind(anchor.height)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn paper_annotations(&self, paper_id: &str) -> Result<Vec<PaperAnnotation>> {
        let annotations: Vec<Annotation> = sqlx::query_as("SELECT a.id,a.citation_id,a.paper_id,a.revision,a.source_message_id,a.kind,a.body,a.state,CASE WHEN p.id IS NULL THEN 'paper-missing' WHEN a.revision=p.canonical_sha256 THEN 'available' ELSE 'revision-stale' END AS availability,a.created_at,a.updated_at FROM annotations a LEFT JOIN papers p ON p.id=a.paper_id WHERE a.paper_id=? ORDER BY a.created_at,a.rowid")
            .bind(paper_id)
            .fetch_all(self.pool())
            .await?;
        let mut result = Vec::with_capacity(annotations.len());
        for annotation in annotations {
            let citation = sqlx::query_as("SELECT id,message_id,paper_id,revision,page,section,locator,quote,prefix,suffix,explanation,match_status,created_at FROM message_citations WHERE id=?")
                .bind(&annotation.citation_id)
                .fetch_one(self.pool())
                .await?;
            let anchors = self.annotation_anchors(&annotation.id).await?;
            result.push(PaperAnnotation {
                annotation,
                citation,
                anchors,
            });
        }
        Ok(result)
    }

    pub async fn append_conversation_event(
        &self,
        conversation_id: &str,
        message_id: Option<&str>,
        event_type: &str,
        payload: &Value,
    ) -> Result<ConversationEvent> {
        let payload_json = serde_json::to_string(payload)?;
        let id = sqlx::query("INSERT INTO conversation_events(conversation_id,message_id,event_type,payload_json) VALUES(?,?,?,?)")
            .bind(conversation_id)
            .bind(message_id)
            .bind(event_type)
            .bind(payload_json)
            .execute(self.pool())
            .await?
            .last_insert_rowid();
        self.conversation_event(id)
            .await?
            .context("created conversation event is missing")
    }

    async fn conversation_event(&self, id: i64) -> Result<Option<ConversationEvent>> {
        let row: Option<(i64, String, Option<String>, String, String, String)> = sqlx::query_as(
            "SELECT id,conversation_id,message_id,event_type,payload_json,created_at FROM conversation_events WHERE id=?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        row.map(event_from_row).transpose()
    }

    pub async fn conversation_events_after(
        &self,
        conversation_id: &str,
        after: i64,
    ) -> Result<Vec<ConversationEvent>> {
        let rows: Vec<(i64, String, Option<String>, String, String, String)> = sqlx::query_as(
            "SELECT id,conversation_id,message_id,event_type,payload_json,created_at FROM conversation_events WHERE conversation_id=? AND id>? ORDER BY id",
        )
        .bind(conversation_id)
        .bind(after.max(0))
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(event_from_row).collect()
    }
}

fn valid_message_status(status: &str) -> bool {
    matches!(
        status,
        "queued" | "running" | "streaming" | "completed" | "failed" | "cancelled" | "interrupted"
    )
}

fn event_from_row(
    (id, conversation_id, message_id, event_type, payload_json, created_at): (
        i64,
        String,
        Option<String>,
        String,
        String,
        String,
    ),
) -> Result<ConversationEvent> {
    Ok(ConversationEvent {
        id,
        conversation_id,
        message_id,
        event_type,
        payload: serde_json::from_str(&payload_json)
            .context("decode conversation event payload")?,
        created_at,
    })
}

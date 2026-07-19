use paper_codex::{db::Database, domain::TaskState, search::SearchIndex};

#[tokio::test]
async fn persists_task_events_and_replays_them_in_order() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let id = db
        .create_task("ingest", r#"{"source":"paper"}"#)
        .await
        .unwrap();
    db.transition_task(&id, TaskState::Resolving, None)
        .await
        .unwrap();
    db.append_event(&id, "stage", r#"{"state":"resolving"}"#)
        .await
        .unwrap();
    db.append_event(&id, "message", r#"{"text":"found"}"#)
        .await
        .unwrap();
    let events = db.events_after(0).await.unwrap();
    assert_eq!(events.len(), 2);
    assert!(events[0].id < events[1].id);
    assert_eq!(db.get_task(&id).await.unwrap().unwrap().state, "resolving");
}

#[tokio::test]
async fn fts_search_is_incremental_and_scopeable() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let search = SearchIndex::new(db.clone());
    search
        .upsert(
            "paper",
            "p1",
            "Attention mechanisms",
            "Transformer architecture",
        )
        .await
        .unwrap();
    search
        .upsert("project", "r1", "Vision", "Image classification")
        .await
        .unwrap();
    let all = search.query("transformer", None).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].entity_id, "p1");
    assert!(search
        .query("image", Some("paper"))
        .await
        .unwrap()
        .is_empty());
}

use paper_codex::{
    db::Database,
    domain::TaskState,
    research::{CandidateStatus, EvidenceLevel, WorkMetadata},
    research_store::ResearchStore,
    search::SearchIndex,
};

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
async fn dismisses_only_terminal_tasks() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let failed = db
        .create_task("ingest", r#"{"source":"failed"}"#)
        .await
        .unwrap();
    db.append_event(&failed, "failed", r#"{"message":"download failed"}"#)
        .await
        .unwrap();
    db.force_task_state(&failed, TaskState::Failed, Some("download failed"))
        .await
        .unwrap();
    let running = db
        .create_task("ingest", r#"{"source":"running"}"#)
        .await
        .unwrap();

    assert!(db.dismiss_task(&failed).await.unwrap());
    assert!(db.get_task(&failed).await.unwrap().is_none());
    assert!(db
        .events_after(0)
        .await
        .unwrap()
        .iter()
        .all(|event| event.task_id != failed));

    let error = db.dismiss_task(&running).await.unwrap_err();
    assert!(error.to_string().contains("terminal"));
    assert!(db.get_task(&running).await.unwrap().is_some());
    assert!(!db.dismiss_task("missing-task").await.unwrap());
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

fn import_work(doi: &str) -> WorkMetadata {
    WorkMetadata {
        canonical_key: format!("doi:{doi}"),
        doi: Some(doi.to_owned()),
        arxiv_id: None,
        openalex_id: None,
        title: "Import candidate".to_owned(),
        authors: Vec::new(),
        year: Some(2024),
        abstract_text: Some("Abstract".to_owned()),
        source_url: format!("https://doi.org/{doi}"),
        pdf_url: None,
        evidence_level: EvidenceLevel::Abstract,
        metadata: serde_json::json!({}),
    }
}

#[tokio::test]
async fn candidate_import_task_completion_and_failure_update_project_state() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let store = ResearchStore::new(db.clone());
    let project = db
        .create_project("candidate", "Candidate", "")
        .await
        .unwrap();
    let completed = store
        .upsert_work(import_work("10.1000/completed"))
        .await
        .unwrap();
    store
        .save_candidate(&project, &completed.id, "相关", &[], None, None)
        .await
        .unwrap();
    let completed_task = db
        .create_task("ingest", r#"{"source":"doi:10.1000/completed"}"#)
        .await
        .unwrap();
    store
        .mark_candidate_importing(&project, &completed.id, &completed_task)
        .await
        .unwrap();
    db.insert_paper("doi:10.1000/completed", "Completed")
        .await
        .unwrap();
    store
        .complete_candidate_import(&completed_task, "doi:10.1000/completed")
        .await
        .unwrap();
    let completed = store
        .get_candidate(&project, &completed.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed.status, CandidateStatus::Imported);
    assert_eq!(completed.paper_id.as_deref(), Some("doi:10.1000/completed"));

    let failed = store
        .upsert_work(import_work("10.1000/failed"))
        .await
        .unwrap();
    store
        .save_candidate(&project, &failed.id, "也相关", &[], None, None)
        .await
        .unwrap();
    let failed_task = db
        .create_task("ingest", r#"{"source":"doi:10.1000/failed"}"#)
        .await
        .unwrap();
    store
        .mark_candidate_importing(&project, &failed.id, &failed_task)
        .await
        .unwrap();
    store.fail_candidate_import(&failed_task).await.unwrap();

    let failed = store
        .get_candidate(&project, &failed.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, CandidateStatus::Candidate);
    assert_eq!(failed.import_task_id, None);
}

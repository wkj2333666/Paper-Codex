use paper_codex::{
    db::Database,
    research::{
        canonical_arxiv_id, canonical_doi, CandidateStatus, EvidenceLevel, ResearchTrigger,
        WorkMetadata,
    },
    research_store::ResearchStore,
};
use serde_json::json;

fn sample_work(canonical_key: &str) -> WorkMetadata {
    WorkMetadata {
        canonical_key: canonical_key.to_owned(),
        doi: canonical_key.strip_prefix("doi:").map(ToOwned::to_owned),
        arxiv_id: canonical_key.strip_prefix("arxiv:").map(ToOwned::to_owned),
        openalex_id: canonical_key
            .strip_prefix("openalex:")
            .map(ToOwned::to_owned),
        title: format!("Paper {canonical_key}"),
        authors: vec!["Ada Lovelace".to_owned()],
        year: Some(2024),
        abstract_text: Some("An independently verified abstract.".to_owned()),
        source_url: "https://example.test/work".to_owned(),
        pdf_url: None,
        evidence_level: EvidenceLevel::Abstract,
        metadata: json!({"fixture": true}),
    }
}

async fn search_context(db: &Database) -> (String, String) {
    let conversation = db.create_conversation("检索").await.unwrap();
    let message = db
        .append_chat_message(&conversation.id, "user", "查找相关工作", "completed")
        .await
        .unwrap();
    (conversation.id, message.id)
}

#[tokio::test]
async fn same_work_has_independent_project_candidate_state() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let store = ResearchStore::new(db.clone());
    let left = db.create_project("left", "左项目", "").await.unwrap();
    let right = db.create_project("right", "右项目", "").await.unwrap();
    let work = store
        .upsert_work(sample_work("doi:10.1000/test"))
        .await
        .unwrap();

    store
        .save_candidate(&left, &work.id, "支持左侧假设", &[], None, None)
        .await
        .unwrap();
    store
        .save_candidate(&right, &work.id, "反驳右侧假设", &[], None, None)
        .await
        .unwrap();
    store
        .set_candidate_status(&left, &work.id, CandidateStatus::Dismissed)
        .await
        .unwrap();

    assert_eq!(
        store
            .get_candidate(&left, &work.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CandidateStatus::Dismissed
    );
    assert_eq!(
        store
            .get_candidate(&right, &work.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        CandidateStatus::Candidate
    );
}

#[tokio::test]
async fn search_run_keeps_results_without_promoting_all_to_candidates() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let store = ResearchStore::new(db.clone());
    let project = db.create_project("bench", "Bench", "").await.unwrap();
    let (conversation, message) = search_context(&db).await;
    let run = store
        .start_search(
            &project,
            &conversation,
            &message,
            ResearchTrigger::Automatic,
            "rule complexity",
        )
        .await
        .unwrap();
    let first = store
        .upsert_work(sample_work("arxiv:2401.00001"))
        .await
        .unwrap();
    let second = store.upsert_work(sample_work("openalex:W2")).await.unwrap();
    store
        .save_search_results(&run.id, "openalex", &[first.clone(), second])
        .await
        .unwrap();
    store
        .save_candidate(
            &project,
            &first.id,
            "直接相关",
            &[],
            Some(&run.id),
            Some(&conversation),
        )
        .await
        .unwrap();

    assert_eq!(store.search_results(&run.id).await.unwrap().len(), 2);
    assert_eq!(
        store
            .list_project_candidates(&project, false)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn failed_import_keeps_an_acquired_paper_imported_but_reopens_unacquired_candidates() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let store = ResearchStore::new(db.clone());
    let project = db.create_project("bench", "Bench", "").await.unwrap();

    let acquired = store
        .upsert_work(sample_work("doi:10.1000/acquired"))
        .await
        .unwrap();
    store
        .save_candidate(&project, &acquired.id, "已获取正式论文", &[], None, None)
        .await
        .unwrap();
    let acquired_task = db
        .create_task("ingest", r#"{"source":"doi:10.1000/acquired"}"#)
        .await
        .unwrap();
    store
        .mark_candidate_importing(&project, &acquired.id, &acquired_task)
        .await
        .unwrap();
    db.insert_paper("paper:acquired", "Acquired paper")
        .await
        .unwrap();
    sqlx::query("UPDATE project_candidates SET paper_id=? WHERE import_task_id=?")
        .bind("paper:acquired")
        .bind(&acquired_task)
        .execute(db.pool())
        .await
        .unwrap();

    let pending = store
        .upsert_work(sample_work("doi:10.1000/pending"))
        .await
        .unwrap();
    store
        .save_candidate(&project, &pending.id, "尚未获取正式论文", &[], None, None)
        .await
        .unwrap();
    let pending_task = db
        .create_task("ingest", r#"{"source":"doi:10.1000/pending"}"#)
        .await
        .unwrap();
    store
        .mark_candidate_importing(&project, &pending.id, &pending_task)
        .await
        .unwrap();

    store.fail_candidate_import(&acquired_task).await.unwrap();
    store.fail_candidate_import(&pending_task).await.unwrap();

    let acquired = store
        .get_candidate(&project, &acquired.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(acquired.status, CandidateStatus::Imported);
    assert_eq!(acquired.paper_id.as_deref(), Some("paper:acquired"));
    assert_eq!(acquired.import_task_id, None);

    let pending = store
        .get_candidate(&project, &pending.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pending.status, CandidateStatus::Candidate);
    assert_eq!(pending.paper_id, None);
    assert_eq!(pending.import_task_id, None);
}

#[tokio::test]
async fn formalization_keeps_enrichment_active_and_repeated_saves_cannot_demote_it() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let store = ResearchStore::new(db.clone());
    let project = db.create_project("formal", "Formal", "").await.unwrap();
    let work = store
        .upsert_work(sample_work("doi:10.1000/formal"))
        .await
        .unwrap();
    store
        .save_candidate(&project, &work.id, "正式导入", &[], None, None)
        .await
        .unwrap();
    let task = db
        .create_task("ingest", r#"{"source":"doi:10.1000/formal"}"#)
        .await
        .unwrap();
    store
        .mark_candidate_importing(&project, &work.id, &task)
        .await
        .unwrap();
    db.insert_paper("paper:formal", "Formal paper")
        .await
        .unwrap();

    assert!(store
        .formalize_project_paper(&task, "paper:formal", &project)
        .await
        .unwrap());
    let formalizing = store
        .get_candidate(&project, &work.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(formalizing.status, CandidateStatus::Importing);
    assert_eq!(formalizing.paper_id.as_deref(), Some("paper:formal"));
    assert_eq!(formalizing.import_task_id.as_deref(), Some(task.as_str()));
    assert_eq!(
        db.paper_project_ids("paper:formal").await.unwrap(),
        vec![project.clone()]
    );

    let repeated = store
        .save_candidate(&project, &work.id, "更新后的理由", &[], None, None)
        .await
        .unwrap();
    assert_eq!(repeated.status, CandidateStatus::Importing);
    assert_eq!(repeated.paper_id.as_deref(), Some("paper:formal"));
    assert_eq!(repeated.import_task_id.as_deref(), Some(task.as_str()));

    assert!(store
        .complete_candidate_import(&task, "paper:formal")
        .await
        .unwrap());
    let imported = store
        .get_candidate(&project, &work.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(imported.status, CandidateStatus::Imported);
    assert_eq!(imported.import_task_id, None);
}

#[tokio::test]
async fn cancelled_task_cannot_formalize_a_project_paper() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let store = ResearchStore::new(db.clone());
    let project = db.create_project("cancel", "Cancel", "").await.unwrap();
    let work = store
        .upsert_work(sample_work("doi:10.1000/cancel"))
        .await
        .unwrap();
    store
        .save_candidate(&project, &work.id, "取消边界", &[], None, None)
        .await
        .unwrap();
    let task = db
        .create_task("ingest", r#"{"source":"doi:10.1000/cancel"}"#)
        .await
        .unwrap();
    store
        .mark_candidate_importing(&project, &work.id, &task)
        .await
        .unwrap();
    db.insert_paper("paper:cancel", "Cancelled paper")
        .await
        .unwrap();
    db.force_task_state(
        &task,
        paper_codex::domain::TaskState::Cancelled,
        None,
    )
    .await
    .unwrap();

    assert!(store
        .formalize_project_paper(&task, "paper:cancel", &project)
        .await
        .is_err());
    assert!(db
        .paper_project_ids("paper:cancel")
        .await
        .unwrap()
        .is_empty());
}

#[test]
fn canonical_identifiers_remove_transport_noise_without_title_merging() {
    assert_eq!(
        canonical_doi(" HTTPS://doi.org/10.1000/Rule. "),
        Some("10.1000/rule".to_owned())
    );
    assert_eq!(
        canonical_arxiv_id("arXiv:2401.01234v3"),
        Some("2401.01234".to_owned())
    );
}

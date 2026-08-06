use async_trait::async_trait;
use paper_codex::{
    acquisition::Acquirer,
    db::Database,
    research::{
        EvidenceLevel, ResearchProvider, ResearchQuery, ResearchTrigger, SearchRunState,
        WorkMetadata,
    },
    research_service::{
        ProjectResearchToolHandler, ProviderState, ResearchService, ResearchServiceConfig,
        SearchRequest,
    },
    research_store::ResearchStore,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tempfile::TempDir;
use tokio::sync::watch;

#[derive(Clone)]
enum StubResponse {
    Works(Vec<WorkMetadata>),
    Error(&'static str),
    Delayed(Vec<WorkMetadata>),
}

#[test]
fn project_research_tools_add_atomically_and_retire_prior_tool_definitions() {
    let definitions = ProjectResearchToolHandler::definitions();
    let add = definitions
        .iter()
        .find(|item| item.name == "research_add_to_project")
        .expect("research_add_to_project tool");
    assert_eq!(
        add
            .input_schema
            .pointer("/required/0")
            .and_then(serde_json::Value::as_str),
        Some("work_id")
    );
    assert!(
        ProjectResearchToolHandler::DEFINITIONS_VERSION > 1,
        "the prior persisted dynamic-tool definition must be invalidated"
    );
}

struct StubProvider {
    name: &'static str,
    response: StubResponse,
}

#[async_trait]
impl ResearchProvider for StubProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn search(&self, _: &ResearchQuery) -> anyhow::Result<Vec<WorkMetadata>> {
        match &self.response {
            StubResponse::Works(works) => Ok(works.clone()),
            StubResponse::Error(error) => Err(anyhow::anyhow!(*error)),
            StubResponse::Delayed(works) => {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok(works.clone())
            }
        }
    }
}

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
        abstract_text: Some("Verified abstract".to_owned()),
        source_url: "https://example.test/work".to_owned(),
        pdf_url: None,
        evidence_level: EvidenceLevel::Abstract,
        metadata: json!({"fixture": canonical_key}),
    }
}

fn stub_ok(name: &'static str, works: Vec<WorkMetadata>) -> Arc<dyn ResearchProvider> {
    Arc::new(StubProvider {
        name,
        response: StubResponse::Works(works),
    })
}

fn stub_error(name: &'static str, error: &'static str) -> Arc<dyn ResearchProvider> {
    Arc::new(StubProvider {
        name,
        response: StubResponse::Error(error),
    })
}

struct ResearchHarness {
    _temp: TempDir,
    store: ResearchStore,
    service: ResearchService,
    project_id: String,
    conversation_id: String,
    message_id: String,
    cache_dir: std::path::PathBuf,
}

impl ResearchHarness {
    async fn new(providers: Vec<Arc<dyn ResearchProvider>>, cache_max_bytes: u64) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let cache_dir = temp.path().join("research-cache");
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let store = ResearchStore::new(db.clone());
        let project_id = db.create_project("research", "Research", "").await.unwrap();
        let conversation = db.create_conversation("检索").await.unwrap();
        let message = db
            .append_chat_message(&conversation.id, "user", "查找相关工作", "completed")
            .await
            .unwrap();
        let service = ResearchService::new(
            store.clone(),
            providers,
            Acquirer::new(1024 * 1024).unwrap(),
            ResearchServiceConfig {
                cache_dir: cache_dir.clone(),
                cache_max_bytes,
                cache_ttl: Duration::from_secs(30 * 24 * 60 * 60),
                max_concurrency: 3,
            },
        )
        .unwrap();
        Self {
            _temp: temp,
            store,
            service,
            project_id,
            conversation_id: conversation.id,
            message_id: message.id,
            cache_dir,
        }
    }

    fn request(&self) -> SearchRequest {
        SearchRequest {
            project_id: self.project_id.clone(),
            conversation_id: self.conversation_id.clone(),
            message_id: self.message_id.clone(),
            trigger: ResearchTrigger::Automatic,
            question: "What work studies rule complexity?".to_owned(),
            query: ResearchQuery {
                text: "rule complexity".to_owned(),
                title_terms: Vec::new(),
                author: None,
                year_from: None,
                year_to: None,
                limit: 10,
            },
        }
    }
}

#[tokio::test]
async fn one_provider_failure_yields_a_partial_search() {
    let harness = ResearchHarness::new(
        vec![
            stub_ok(
                "openalex",
                vec![
                    sample_work("doi:10.1000/one"),
                    sample_work("doi:10.1000/two"),
                ],
            ),
            stub_error("crossref", "timeout"),
        ],
        1024 * 1024,
    )
    .await;

    let outcome = harness.service.search(harness.request()).await.unwrap();

    assert_eq!(outcome.state, SearchRunState::Partial);
    assert_eq!(outcome.works.len(), 2);
    assert_eq!(outcome.providers["crossref"].state, ProviderState::Failed);
}

#[tokio::test]
async fn all_provider_failures_persist_a_failed_search_with_its_run_id() {
    let harness = ResearchHarness::new(
        vec![
            stub_error("openalex", "timeout"),
            stub_error("crossref", "bad gateway"),
        ],
        1024 * 1024,
    )
    .await;

    let error = harness.service.search(harness.request()).await.unwrap_err();
    let searches = harness
        .store
        .list_project_searches(&harness.project_id)
        .await
        .unwrap();

    assert!(error.to_string().contains(&searches[0].id));
    assert_eq!(searches[0].state, SearchRunState::Failed);
}

#[tokio::test]
async fn duplicate_provider_results_become_one_search_result() {
    let work = sample_work("doi:10.1000/shared");
    let harness = ResearchHarness::new(
        vec![
            stub_ok("openalex", vec![work.clone()]),
            stub_ok("crossref", vec![work]),
        ],
        1024 * 1024,
    )
    .await;

    let outcome = harness.service.search(harness.request()).await.unwrap();
    let results = harness.store.search_results(&outcome.run_id).await.unwrap();

    assert_eq!(outcome.works.len(), 1);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].providers, vec!["crossref", "openalex"]);
}

#[tokio::test]
async fn cancellation_marks_the_search_run_cancelled() {
    let provider: Arc<dyn ResearchProvider> = Arc::new(StubProvider {
        name: "slow",
        response: StubResponse::Delayed(vec![sample_work("doi:10.1000/slow")]),
    });
    let harness = ResearchHarness::new(vec![provider], 1024 * 1024).await;
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let service = harness.service.clone();
    let request = harness.request();
    let search = tokio::spawn(async move { service.search_with_cancel(request, cancel_rx).await });
    tokio::task::yield_now().await;
    cancel_tx.send(true).unwrap();

    let outcome = search.await.unwrap().unwrap();

    assert_eq!(outcome.state, SearchRunState::Cancelled);
    assert_eq!(
        harness
            .store
            .get_search(&outcome.run_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        SearchRunState::Cancelled
    );
}

#[tokio::test]
async fn abstract_inspection_does_not_fetch_a_pdf() {
    let harness = ResearchHarness::new(Vec::new(), 1024 * 1024).await;
    let work = harness
        .store
        .upsert_work(sample_work("doi:10.1000/abstract"))
        .await
        .unwrap();

    let outcome = harness.service.inspect(&work.id, false).await.unwrap();

    assert_eq!(outcome.evidence_level, EvidenceLevel::Abstract);
    assert_eq!(outcome.text, "Verified abstract");
    assert!(!harness
        .cache_dir
        .join("works")
        .join(&work.id)
        .join("source.pdf")
        .exists());
}

async fn start_file_server(body: Vec<u8>) -> (String, tokio::task::JoinHandle<()>) {
    let app = axum::Router::new().route(
        "/paper.pdf",
        axum::routing::get(move || {
            let body = body.clone();
            async move { body }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}/paper.pdf"), server)
}

#[tokio::test]
async fn valid_pdf_inspection_uses_the_bounded_fulltext_cache() {
    let bytes = b"%PDF-1.7\ncached fixture".to_vec();
    let (url, server) = start_file_server(bytes.clone()).await;
    let harness = ResearchHarness::new(Vec::new(), 1024 * 1024).await;
    let mut metadata = sample_work("doi:10.1000/fulltext");
    metadata.pdf_url = Some(url);
    let work = harness.store.upsert_work(metadata).await.unwrap();
    let sha = hex::encode(Sha256::digest(&bytes));
    let extraction_cache = harness
        .cache_dir
        .join("works")
        .join(&work.id)
        .join("extraction")
        .join(sha);
    tokio::fs::create_dir_all(&extraction_cache).await.unwrap();
    tokio::fs::write(
        extraction_cache.join("pages.json"),
        serde_json::to_vec(&vec!["Full text evidence"]).unwrap(),
    )
    .await
    .unwrap();

    let outcome = harness.service.inspect(&work.id, true).await.unwrap();

    assert_eq!(outcome.evidence_level, EvidenceLevel::Fulltext);
    assert!(outcome.text.contains("Full text evidence"));
    assert!(harness
        .cache_dir
        .join("works")
        .join(&work.id)
        .join("extracted.md")
        .exists());
    server.abort();
}

#[tokio::test]
async fn invalid_pdf_is_rejected_without_upgrading_evidence() {
    let (url, server) = start_file_server(b"<html>not a pdf</html>".to_vec()).await;
    let harness = ResearchHarness::new(Vec::new(), 1024 * 1024).await;
    let mut metadata = sample_work("doi:10.1000/invalid");
    metadata.pdf_url = Some(url);
    let work = harness.store.upsert_work(metadata).await.unwrap();

    let error = harness.service.inspect(&work.id, true).await.unwrap_err();
    let stored = harness.store.get_work(&work.id).await.unwrap().unwrap();

    assert!(error.to_string().contains("not a PDF"));
    assert_eq!(stored.metadata.evidence_level, EvidenceLevel::Abstract);
    server.abort();
}

#[tokio::test]
async fn cache_pruning_removes_artifacts_but_retains_discovered_metadata() {
    let harness = ResearchHarness::new(Vec::new(), 4).await;
    let work = harness
        .store
        .upsert_work(sample_work("doi:10.1000/prune"))
        .await
        .unwrap();
    let work_dir = harness.cache_dir.join("works").join(&work.id);
    tokio::fs::create_dir_all(&work_dir).await.unwrap();
    tokio::fs::write(work_dir.join("source.pdf"), b"12345678")
        .await
        .unwrap();
    tokio::fs::write(work_dir.join("extracted.md"), b"abcdefgh")
        .await
        .unwrap();

    let outcome = harness.service.prune_cache().await.unwrap();

    assert!(outcome.removed_bytes >= 12);
    assert!(harness.store.get_work(&work.id).await.unwrap().is_some());
}

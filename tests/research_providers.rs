use axum::{
    extract::State,
    http::{header::RETRY_AFTER, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use paper_codex::{
    research::{EvidenceLevel, ResearchProvider, ResearchQuery},
    research_providers::{
        parse_arxiv_search, parse_crossref_search, parse_openalex_search, OpenAlexProvider,
    },
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::net::TcpListener;
use url::Url;

#[test]
fn openalex_inverted_index_becomes_an_abstract() {
    let response = include_str!("../fixtures/research/openalex-search.json");
    let works = parse_openalex_search(response).unwrap();

    assert_eq!(works.len(), 2);
    assert_eq!(works[0].canonical_key, "doi:10.1000/rule");
    assert_eq!(
        works[0].abstract_text.as_deref(),
        Some("Rule complexity for games")
    );
    assert_eq!(works[0].evidence_level, EvidenceLevel::Abstract);
    assert_eq!(works[1].canonical_key, "openalex:W200");
    assert_eq!(works[1].evidence_level, EvidenceLevel::Metadata);
}

#[test]
fn crossref_claims_a_pdf_only_for_an_explicit_pdf_link() {
    let response = include_str!("../fixtures/research/crossref-search.json");
    let works = parse_crossref_search(response).unwrap();

    assert_eq!(works.len(), 2);
    assert_eq!(works[0].canonical_key, "doi:10.1000/rule");
    assert_eq!(
        works[0].abstract_text.as_deref(),
        Some("Rule complexity for games")
    );
    assert_eq!(
        works[0].pdf_url.as_deref(),
        Some("https://example.test/rule.pdf")
    );
    assert_eq!(works[1].pdf_url, None);
    assert_eq!(works[1].evidence_level, EvidenceLevel::Metadata);
}

#[test]
fn arxiv_versions_normalize_to_one_identifier() {
    let response = include_str!("../fixtures/research/arxiv-search.xml");
    let works = parse_arxiv_search(response).unwrap();

    assert_eq!(works.len(), 2);
    assert_eq!(works[0].arxiv_id.as_deref(), Some("2401.01234"));
    assert_eq!(works[0].canonical_key, "arxiv:2401.01234");
    assert_eq!(works[0].metadata["published"], "2024-01-02T00:00:00Z");
    assert_eq!(works[0].metadata["updated"], "2024-01-05T00:00:00Z");
    assert_eq!(works[1].arxiv_id.as_deref(), Some("cs/9901001"));
}

#[test]
fn arxiv_atom_allows_interleaved_links_and_authors() {
    let response = r#"<feed xmlns="http://www.w3.org/2005/Atom" xmlns:arxiv="http://arxiv.org/schemas/atom">
      <entry>
        <id>http://arxiv.org/abs/2609.30264v2</id>
        <link href="https://arxiv.org/abs/2609.30264v2" rel="alternate"/>
        <title>Interleaved Atom example</title>
        <author><name>First Author</name></author>
        <updated>2026-09-24T17:59:41Z</updated>
        <summary>A deterministic metadata fixture.</summary>
        <category term="cs.RO"/>
        <link href="https://arxiv.org/pdf/2609.30264v2" rel="related" type="application/pdf"/>
        <published>2026-09-23T17:59:41Z</published>
        <author><name>Second Author</name></author>
        <arxiv:doi>10.1000/example</arxiv:doi>
        <link href="https://doi.org/10.1000/example" rel="related"/>
      </entry>
    </feed>"#;
    let works = parse_arxiv_search(response).unwrap();
    assert_eq!(works.len(), 1);
    assert_eq!(works[0].authors, ["First Author", "Second Author"]);
    assert_eq!(works[0].metadata["links"].as_array().unwrap().len(), 3);
    assert_eq!(works[0].metadata["published"], "2026-09-23T17:59:41Z");
    assert_eq!(works[0].metadata["updated"], "2026-09-24T17:59:41Z");
}

#[test]
fn arxiv_does_not_silently_accept_an_html_error_page() {
    assert!(parse_arxiv_search("<html><body>Service unavailable</body></html>").is_err());
    assert!(parse_arxiv_search("").is_err());
    assert!(
        parse_arxiv_search("<feed xmlns='http://www.w3.org/2005/Atom'/>")
            .unwrap()
            .is_empty()
    );
}

#[derive(Clone)]
struct RetryState {
    attempts: Arc<AtomicUsize>,
}

async fn retry_once(State(state): State<RetryState>) -> Response {
    let attempt = state.attempts.fetch_add(1, Ordering::SeqCst);
    if attempt == 0 {
        let mut response = StatusCode::TOO_MANY_REQUESTS.into_response();
        response
            .headers_mut()
            .insert(RETRY_AFTER, HeaderValue::from_static("0"));
        response
    } else {
        (
            StatusCode::OK,
            [("content-type", "application/json")],
            include_str!("../fixtures/research/openalex-search.json"),
        )
            .into_response()
    }
}

async fn bad_request(State(state): State<RetryState>) -> Response {
    state.attempts.fetch_add(1, Ordering::SeqCst);
    StatusCode::BAD_REQUEST.into_response()
}

async fn serve(handler: Router, attempts: Arc<AtomicUsize>) -> (Url, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, handler).await.unwrap();
    });
    (Url::parse(&format!("http://{address}/")).unwrap(), attempts)
}

fn query() -> ResearchQuery {
    ResearchQuery {
        text: "rule complexity".to_owned(),
        title_terms: Vec::new(),
        author: None,
        year_from: None,
        year_to: None,
        limit: 10,
    }
}

#[tokio::test]
async fn provider_retries_one_rate_limit_response() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let state = RetryState {
        attempts: attempts.clone(),
    };
    let router = Router::new()
        .route("/works", get(retry_once))
        .with_state(state);
    let (base_url, attempts) = serve(router, attempts).await;
    let provider = OpenAlexProvider::new(reqwest::Client::new(), base_url);

    let works = provider.search(&query()).await.unwrap();

    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(works.len(), 2);
}

#[tokio::test]
async fn provider_does_not_retry_an_invalid_request() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let state = RetryState {
        attempts: attempts.clone(),
    };
    let router = Router::new()
        .route("/works", get(bad_request))
        .with_state(state);
    let (base_url, attempts) = serve(router, attempts).await;
    let provider = OpenAlexProvider::new(reqwest::Client::new(), base_url);

    let error = provider.search(&query()).await.unwrap_err();

    assert!(error.to_string().contains("400"));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

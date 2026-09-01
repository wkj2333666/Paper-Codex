use paper_codex::{
    acquisition::{classify_input, validate_pdf_bytes, Acquirer, IntakeKind, PdfSource},
    extraction::{extract_pdf, pages_as_markdown},
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::io::AsyncWriteExt;

async fn flaky_pdf(
    axum::extract::State(attempts): axum::extract::State<Arc<AtomicUsize>>,
) -> (axum::http::StatusCode, Vec<u8>) {
    if attempts.fetch_add(1, Ordering::SeqCst) < 2 {
        (axum::http::StatusCode::SERVICE_UNAVAILABLE, vec![])
    } else {
        (axum::http::StatusCode::OK, b"%PDF-1.7\nbody".to_vec())
    }
}

async fn browser_challenge() -> (axum::http::StatusCode, &'static str) {
    (
        axum::http::StatusCode::FORBIDDEN,
        "Challenge verification required",
    )
}

async fn missing_pdf() -> axum::http::StatusCode {
    axum::http::StatusCode::NOT_FOUND
}

async fn valid_fallback_pdf() -> Vec<u8> {
    b"%PDF-1.7\nfallback".to_vec()
}

#[test]
fn classifies_supported_intake_values() {
    assert!(matches!(classify_input("10.1000/xyz"), IntakeKind::Doi(_)));
    assert!(matches!(
        classify_input("https://arxiv.org/abs/1706.03762v5"),
        IntakeKind::Arxiv(_)
    ));
    assert!(matches!(
        classify_input("https://example.org/paper.pdf"),
        IntakeKind::Url(_)
    ));
    assert!(matches!(
        classify_input("Attention Is All You Need"),
        IntakeKind::Title(_)
    ));
}

#[test]
fn arxiv_doi_is_classified_as_arxiv_instead_of_crossref_doi() {
    assert_eq!(
        classify_input("10.48550/arXiv.2603.10098"),
        IntakeKind::Arxiv("2603.10098".into())
    );
    assert_eq!(
        classify_input("https://doi.org/10.48550/arXiv.2603.10098"),
        IntakeKind::Arxiv("2603.10098".into())
    );
}

#[test]
fn rejects_non_pdf_and_oversized_pdf_bytes() {
    assert!(validate_pdf_bytes(b"<html>not pdf</html>", 1024).is_err());
    assert!(validate_pdf_bytes(b"%PDF-1.7\nbody", 8).is_err());
    assert!(validate_pdf_bytes(b"%PDF-1.7\nbody", 1024).is_ok());
}

#[test]
fn acquirer_applies_its_configured_limit_to_uploaded_pdfs() {
    let acquirer = Acquirer::new(12).unwrap();
    assert!(acquirer.validate_pdf(b"%PDF-1.7\nok").is_ok());
    assert!(acquirer.validate_pdf(b"%PDF-1.7\ntoo-long").is_err());
}

#[tokio::test]
async fn pdf_download_retries_transient_server_failures() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let app = axum::Router::new()
        .route("/paper.pdf", axum::routing::get(flaky_pdf))
        .with_state(attempts.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let bytes = Acquirer::new(1024)
        .unwrap()
        .download_pdf(&format!("http://{address}/paper.pdf"))
        .await
        .unwrap();

    assert_eq!(bytes, b"%PDF-1.7\nbody");
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    server.abort();
}

#[tokio::test]
async fn pdf_download_retries_dropped_connections() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for attempt in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            if attempt < 2 {
                drop(stream);
                continue;
            }
            let body = b"%PDF-1.7\nbody";
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(headers.as_bytes()).await.unwrap();
            stream.write_all(body).await.unwrap();
        }
    });

    let bytes = Acquirer::new(1024)
        .unwrap()
        .download_pdf(&format!("http://{address}/paper.pdf"))
        .await
        .unwrap();

    assert_eq!(bytes, b"%PDF-1.7\nbody");
    server.await.unwrap();
}

#[tokio::test]
async fn pdf_source_chain_falls_back_after_an_openreview_browser_challenge() {
    let app = axum::Router::new()
        .route("/openreview", axum::routing::get(browser_challenge))
        .route("/mirror.pdf", axum::routing::get(valid_fallback_pdf));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let downloaded = Acquirer::new(1024)
        .unwrap()
        .download_pdf_sources(
            &[
                PdfSource {
                    provider: "openreview".to_owned(),
                    url: format!("http://{address}/openreview"),
                },
                PdfSource {
                    provider: "mirror".to_owned(),
                    url: format!("http://{address}/mirror.pdf"),
                },
            ],
            cancel_rx,
        )
        .await
        .unwrap();

    assert_eq!(downloaded.bytes, b"%PDF-1.7\nfallback");
    assert_eq!(downloaded.source.provider, "mirror");
    server.abort();
}

#[tokio::test]
async fn exhausted_pdf_sources_report_safe_structured_attempts() {
    let app = axum::Router::new()
        .route("/openreview", axum::routing::get(browser_challenge))
        .route("/missing.pdf", axum::routing::get(missing_pdf));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let error = Acquirer::new(1024)
        .unwrap()
        .download_pdf_sources(
            &[
                PdfSource {
                    provider: "openreview".to_owned(),
                    url: format!("http://{address}/openreview?token=must-not-leak"),
                },
                PdfSource {
                    provider: "mirror".to_owned(),
                    url: format!("http://{address}/missing.pdf"),
                },
            ],
            cancel_rx,
        )
        .await
        .unwrap_err();

    assert_eq!(error.attempts.len(), 2);
    assert_eq!(error.attempts[0].status, Some(403));
    assert_eq!(error.attempts[0].reason_code, "browser_challenge_required");
    assert_eq!(error.attempts[1].status, Some(404));
    let serialized = serde_json::to_string(&error.attempts).unwrap();
    assert!(!serialized.contains("must-not-leak"));
    assert!(!serialized.contains("Challenge verification required"));
    server.abort();
}

#[test]
fn extracted_pages_keep_one_based_evidence_markers() {
    let markdown = pages_as_markdown(&["Abstract".into(), "Method".into()]);
    assert!(markdown.contains("<!-- page:1 -->\nAbstract"));
    assert!(markdown.contains("<!-- page:2 -->\nMethod"));
}

#[tokio::test]
async fn cached_extraction_backfills_revision_markdown() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    let cache_dir = cache.join("extraction/revision-one");
    tokio::fs::create_dir_all(&cache_dir).await.unwrap();
    tokio::fs::write(
        cache_dir.join("pages.json"),
        serde_json::to_vec(&vec!["Abstract", "Method"]).unwrap(),
    )
    .await
    .unwrap();

    let extracted = extract_pdf(
        &temp.path().join("paper-does-not-need-to-exist.pdf"),
        &cache,
        "revision-one",
    )
    .await
    .unwrap();

    assert_eq!(extracted.markdown_path, cache_dir.join("pages.md"));
    assert_eq!(
        tokio::fs::read_to_string(&extracted.markdown_path)
            .await
            .unwrap(),
        extracted.markdown
    );
}

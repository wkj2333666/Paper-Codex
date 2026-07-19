use paper_codex::{
    acquisition::{classify_input, validate_pdf_bytes, Acquirer, IntakeKind},
    extraction::pages_as_markdown,
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

#[test]
fn extracted_pages_keep_one_based_evidence_markers() {
    let markdown = pages_as_markdown(&["Abstract".into(), "Method".into()]);
    assert!(markdown.contains("<!-- page:1 -->\nAbstract"));
    assert!(markdown.contains("<!-- page:2 -->\nMethod"));
}

use crate::domain::PaperIdentity;
use anyhow::{bail, Context, Result};
use futures::StreamExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntakeKind {
    Doi(String),
    Arxiv(String),
    Url(Url),
    Title(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPaper {
    pub identity: Option<String>,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i64>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub source_url: String,
    pub pdf_url: String,
}

pub fn classify_input(input: &str) -> IntakeKind {
    let value = input.trim();
    if let Some(identity) = PaperIdentity::parse(value) {
        return match identity {
            PaperIdentity::Doi(doi) => IntakeKind::Doi(doi),
            PaperIdentity::Arxiv(id) => IntakeKind::Arxiv(id),
            PaperIdentity::Sha256(_) => IntakeKind::Title(value.to_owned()),
        };
    }
    Url::parse(value)
        .map(IntakeKind::Url)
        .unwrap_or_else(|_| IntakeKind::Title(value.to_owned()))
}

pub fn validate_pdf_bytes(bytes: &[u8], max_bytes: usize) -> Result<()> {
    if bytes.len() > max_bytes {
        bail!("PDF exceeds configured upload limit");
    }
    if !bytes.starts_with(b"%PDF-") {
        bail!("content is not a PDF");
    }
    Ok(())
}

#[derive(Clone)]
pub struct Acquirer {
    client: reqwest::Client,
    max_bytes: usize,
}

impl Acquirer {
    pub fn new(max_bytes: usize) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .user_agent("PaperCodex/0.1 (research workspace)")
                .redirect(reqwest::redirect::Policy::limited(8))
                .build()?,
            max_bytes,
        })
    }

    pub async fn resolve(&self, input: &str) -> Result<ResolvedPaper> {
        match classify_input(input) {
            IntakeKind::Doi(doi) => self.resolve_doi(&doi).await,
            IntakeKind::Arxiv(id) => Ok(resolved_arxiv(&id)),
            IntakeKind::Title(title) => self.resolve_title(&title).await,
            IntakeKind::Url(url) => self.resolve_url(url).await,
        }
    }

    pub fn validate_pdf(&self, bytes: &[u8]) -> Result<()> {
        validate_pdf_bytes(bytes, self.max_bytes)
    }

    async fn crossref(&self, url: &str, query: Option<&str>) -> Result<Value> {
        let mut request = self.client.get(url);
        if let Some(title) = query {
            request = request.query(&[("query.title", title), ("rows", "1")]);
        }
        let value: Value = request.send().await?.error_for_status()?.json().await?;
        if query.is_some() {
            value
                .pointer("/message/items/0")
                .cloned()
                .context("Crossref returned no title match")
        } else {
            value
                .get("message")
                .cloned()
                .context("Crossref response lacks message")
        }
    }

    async fn resolve_doi(&self, doi: &str) -> Result<ResolvedPaper> {
        let item = self
            .crossref(
                &format!(
                    "https://api.crossref.org/works/{}",
                    urlencoding::encode(doi)
                ),
                None,
            )
            .await?;
        self.crossref_item(item, Some(doi)).await
    }

    async fn resolve_title(&self, title: &str) -> Result<ResolvedPaper> {
        let item = self
            .crossref("https://api.crossref.org/works", Some(title))
            .await?;
        self.crossref_item(item, None).await
    }

    async fn crossref_item(
        &self,
        item: Value,
        fallback_doi: Option<&str>,
    ) -> Result<ResolvedPaper> {
        let doi = item
            .get("DOI")
            .and_then(Value::as_str)
            .or(fallback_doi)
            .map(|v| v.to_ascii_lowercase());
        let title = item
            .pointer("/title/0")
            .and_then(Value::as_str)
            .unwrap_or("Untitled paper")
            .to_owned();
        let authors = item
            .get("author")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|author| {
                format!(
                    "{} {}",
                    author.get("given").and_then(Value::as_str).unwrap_or(""),
                    author.get("family").and_then(Value::as_str).unwrap_or("")
                )
                .trim()
                .to_owned()
            })
            .filter(|v| !v.is_empty())
            .collect();
        let year = item
            .pointer("/published/date-parts/0/0")
            .and_then(Value::as_i64);
        let source_url = item
            .get("URL")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let mut pdf_url = item
            .get("link")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|link| {
                link.get("content-type").and_then(Value::as_str) == Some("application/pdf")
            })
            .and_then(|link| link.get("URL"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        if pdf_url.is_empty() {
            if let Some(doi_value) = &doi {
                pdf_url = self.openalex_pdf(doi_value).await.unwrap_or_default();
            }
        }
        if pdf_url.is_empty() {
            bail!("metadata resolved, but no downloadable PDF was found");
        }
        Ok(ResolvedPaper {
            identity: doi.as_ref().map(|v| format!("doi:{v}")),
            title,
            authors,
            year,
            doi,
            arxiv_id: None,
            source_url,
            pdf_url,
        })
    }

    async fn openalex_pdf(&self, doi: &str) -> Result<String> {
        let value: Value = self
            .client
            .get(format!(
                "https://api.openalex.org/works/https://doi.org/{doi}"
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        value
            .pointer("/best_oa_location/pdf_url")
            .or_else(|| value.pointer("/open_access/oa_url"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .context("OpenAlex has no open PDF")
    }

    async fn resolve_url(&self, url: Url) -> Result<ResolvedPaper> {
        if let Some(identity) = PaperIdentity::parse(url.as_str()) {
            return match identity {
                PaperIdentity::Doi(doi) => self.resolve_doi(&doi).await,
                PaperIdentity::Arxiv(id) => Ok(resolved_arxiv(&id)),
                PaperIdentity::Sha256(_) => unreachable!(),
            };
        }
        if url.path().to_ascii_lowercase().ends_with(".pdf") {
            return Ok(ResolvedPaper {
                identity: None,
                title: url
                    .path_segments()
                    .and_then(Iterator::last)
                    .unwrap_or("paper.pdf")
                    .to_owned(),
                authors: vec![],
                year: None,
                doi: None,
                arxiv_id: None,
                source_url: url.to_string(),
                pdf_url: url.to_string(),
            });
        }
        let html = self
            .client
            .get(url.clone())
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let meta = |name: &str| -> Option<String> {
            let pattern = format!(
                r#"(?is)<meta[^>]+name=[\"']{}[\"'][^>]+content=[\"']([^\"']+)[\"']"#,
                regex::escape(name)
            );
            Regex::new(&pattern)
                .ok()?
                .captures(&html)?
                .get(1)
                .map(|m| m.as_str().to_owned())
        };
        if let Some(doi) = meta("citation_doi") {
            return self.resolve_doi(&doi).await;
        }
        let pdf_url = meta("citation_pdf_url")
            .context("paper page has no citation_pdf_url or DOI metadata")?;
        Ok(ResolvedPaper {
            identity: None,
            title: meta("citation_title").unwrap_or_else(|| "Untitled paper".into()),
            authors: vec![],
            year: None,
            doi: None,
            arxiv_id: None,
            source_url: url.to_string(),
            pdf_url: url.join(&pdf_url).map(|u| u.to_string()).unwrap_or(pdf_url),
        })
    }

    pub async fn download_pdf(&self, url: &str) -> Result<Vec<u8>> {
        const MAX_ATTEMPTS: usize = 3;
        for attempt in 0..MAX_ATTEMPTS {
            match self.download_pdf_once(url).await {
                Ok(bytes) => return Ok(bytes),
                Err(error) if attempt + 1 < MAX_ATTEMPTS && retryable_download_error(&error) => {
                    let delay = Duration::from_millis(500 * (1_u64 << attempt));
                    tracing::warn!(
                        url,
                        attempt = attempt + 1,
                        retry_in_ms = delay.as_millis(),
                        error = %error,
                        "transient PDF download failure"
                    );
                    tokio::time::sleep(delay).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("download loop always returns on its final attempt")
    }

    async fn download_pdf_once(&self, url: &str) -> Result<Vec<u8>> {
        let response = self.client.get(url).send().await?.error_for_status()?;
        if response
            .content_length()
            .is_some_and(|n| n > self.max_bytes as u64)
        {
            bail!("PDF exceeds configured download limit");
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(&chunk?);
            if bytes.len() > self.max_bytes {
                bail!("PDF exceeds configured download limit");
            }
        }
        validate_pdf_bytes(&bytes, self.max_bytes)?;
        Ok(bytes)
    }
}

fn retryable_download_error(error: &anyhow::Error) -> bool {
    let Some(error) = error.downcast_ref::<reqwest::Error>() else {
        return false;
    };
    if let Some(status) = error.status() {
        return status == reqwest::StatusCode::REQUEST_TIMEOUT
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status.is_server_error();
    }
    error.is_connect() || error.is_timeout() || error.is_request() || error.is_body()
}

fn resolved_arxiv(id: &str) -> ResolvedPaper {
    ResolvedPaper {
        identity: Some(format!("arxiv:{id}")),
        title: format!("arXiv:{id}"),
        authors: vec![],
        year: None,
        doi: None,
        arxiv_id: Some(id.to_owned()),
        source_url: format!("https://arxiv.org/abs/{id}"),
        pdf_url: format!("https://arxiv.org/pdf/{id}.pdf"),
    }
}

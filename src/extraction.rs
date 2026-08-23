use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedPaper {
    pub pages: Vec<String>,
    pub markdown: String,
    pub cache_path: PathBuf,
    pub markdown_path: PathBuf,
}

#[derive(Debug)]
struct ExtractedPages {
    pages: Vec<String>,
    extractor: &'static str,
    fallback_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExtractionMetadata<'a> {
    extractor: &'a str,
    fallback_reason: Option<&'a str>,
}

pub fn pages_as_markdown(pages: &[String]) -> String {
    pages
        .iter()
        .enumerate()
        .map(|(index, page)| format!("<!-- page:{} -->\n{}", index + 1, page.trim()))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

fn pages_contain_visible_text(pages: &[String]) -> bool {
    !pages.is_empty() && pages.iter().any(|page| !page.trim().is_empty())
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else {
        "unknown panic payload".to_string()
    }
}

fn extract_pages_with_fallback<Primary, Fallback>(
    primary: Primary,
    fallback: Fallback,
) -> Result<ExtractedPages>
where
    Primary: FnOnce() -> Result<Vec<String>>,
    Fallback: FnOnce() -> Result<Vec<String>>,
{
    let primary_failure = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(primary)) {
        Ok(Ok(pages)) if pages_contain_visible_text(&pages) => {
            return Ok(ExtractedPages {
                pages,
                extractor: "pdf-extract",
                fallback_reason: None,
            });
        }
        Ok(Ok(_)) => "pdf-extract produced no visible text".to_string(),
        Ok(Err(error)) => format!("pdf-extract failed: {error:#}"),
        Err(payload) => format!("pdf-extract panicked: {}", panic_message(payload.as_ref())),
    };

    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(fallback)) {
        Ok(Ok(pages)) if pages_contain_visible_text(&pages) => Ok(ExtractedPages {
            pages,
            extractor: "pdf-oxide",
            fallback_reason: Some(primary_failure),
        }),
        Ok(Ok(_)) => bail!(
            "both PDF extractors failed; {primary_failure}; pdf-oxide produced no visible text"
        ),
        Ok(Err(error)) => {
            bail!("both PDF extractors failed; {primary_failure}; pdf-oxide failed: {error:#}")
        }
        Err(payload) => bail!(
            "both PDF extractors failed; {primary_failure}; pdf-oxide panicked: {}",
            panic_message(payload.as_ref())
        ),
    }
}

fn extract_pages_with_pdf_extract(path: &Path) -> Result<Vec<String>> {
    let expected_page_count = pdf_extract::Document::load(path)
        .with_context(|| format!("open PDF for page-count validation: {}", path.display()))?
        .get_pages()
        .len();
    let pages = pdf_extract::extract_text_by_pages(path).context("extract PDF with pdf-extract")?;
    if pages.len() != expected_page_count {
        bail!(
            "pdf-extract returned {} of {expected_page_count} pages",
            pages.len()
        );
    }
    Ok(pages)
}

fn extract_pages_with_pdf_oxide(path: &Path) -> Result<Vec<String>> {
    let document = pdf_oxide::PdfDocument::open(path)
        .with_context(|| format!("open PDF with pdf-oxide: {}", path.display()))?;
    let page_count = document
        .page_count()
        .context("read PDF page count with pdf-oxide")?;
    let mut pages = Vec::with_capacity(page_count);
    for page_index in 0..page_count {
        pages.push(
            document
                .extract_text(page_index)
                .with_context(|| format!("extract PDF page {} with pdf-oxide", page_index + 1))?,
        );
    }
    Ok(pages)
}

pub async fn extract_pdf(path: &Path, cache_root: &Path, sha256: &str) -> Result<ExtractedPaper> {
    let cache_dir = cache_root.join("extraction").join(sha256);
    let cache_path = cache_dir.join("pages.json");
    let markdown_path = cache_dir.join("pages.md");
    if let Ok(bytes) = tokio::fs::read(&cache_path).await {
        let pages: Vec<String> =
            serde_json::from_slice(&bytes).context("decode cached PDF pages")?;
        let markdown = pages_as_markdown(&pages);
        if tokio::fs::metadata(&markdown_path).await.is_err() {
            crate::workspace::atomic_write(&markdown_path, markdown.as_bytes()).await?;
        }
        return Ok(ExtractedPaper {
            markdown,
            pages,
            cache_path,
            markdown_path,
        });
    }
    let primary_source = path.to_path_buf();
    let fallback_source = primary_source.clone();
    let extracted = tokio::task::spawn_blocking(move || {
        extract_pages_with_fallback(
            move || extract_pages_with_pdf_extract(&primary_source),
            move || extract_pages_with_pdf_oxide(&fallback_source),
        )
    })
    .await
    .context("join PDF extraction coordinator")??;
    if let Some(reason) = extracted.fallback_reason.as_deref() {
        tracing::warn!(
            reason,
            "primary PDF extraction failed; used pdf-oxide fallback"
        );
    }
    let metadata = ExtractionMetadata {
        extractor: extracted.extractor,
        fallback_reason: extracted.fallback_reason.as_deref(),
    };
    let pages = extracted.pages;
    tokio::fs::create_dir_all(&cache_dir).await?;
    crate::workspace::atomic_write(&cache_path, &serde_json::to_vec(&pages)?).await?;
    crate::workspace::atomic_write(
        &cache_dir.join("metadata.json"),
        &serde_json::to_vec_pretty(&metadata)?,
    )
    .await?;
    let markdown = pages_as_markdown(&pages);
    crate::workspace::atomic_write(&markdown_path, markdown.as_bytes()).await?;
    Ok(ExtractedPaper {
        markdown,
        pages,
        cache_path,
        markdown_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[test]
    fn successful_primary_extraction_keeps_existing_result() {
        let fallback_called = Arc::new(AtomicBool::new(false));
        let fallback_probe = Arc::clone(&fallback_called);

        let extracted = extract_pages_with_fallback(
            || Ok(vec!["primary page".to_string()]),
            move || {
                fallback_probe.store(true, Ordering::SeqCst);
                Ok(vec!["fallback page".to_string()])
            },
        )
        .unwrap();

        assert_eq!(extracted.pages, vec!["primary page"]);
        assert_eq!(extracted.extractor, "pdf-extract");
        assert!(extracted.fallback_reason.is_none());
        assert!(!fallback_called.load(Ordering::SeqCst));
    }

    #[test]
    fn primary_panic_uses_pdf_oxide_fallback() {
        let extracted = extract_pages_with_fallback(
            || -> Result<Vec<String>> { panic!("missing width for Type3 font") },
            || Ok(vec!["fallback page".to_string()]),
        )
        .unwrap();

        assert_eq!(extracted.pages, vec!["fallback page"]);
        assert_eq!(extracted.extractor, "pdf-oxide");
        assert!(extracted
            .fallback_reason
            .as_deref()
            .unwrap()
            .contains("missing width for Type3 font"));
    }

    #[test]
    fn both_extractor_failures_are_reported() {
        let error = extract_pages_with_fallback(
            || anyhow::bail!("primary font failure"),
            || anyhow::bail!("fallback parse failure"),
        )
        .unwrap_err();
        let message = format!("{error:#}");

        assert!(message.contains("primary font failure"));
        assert!(message.contains("fallback parse failure"));
    }
}

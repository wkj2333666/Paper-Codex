use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedPaper {
    pub pages: Vec<String>,
    pub markdown: String,
    pub cache_path: PathBuf,
}

pub fn pages_as_markdown(pages: &[String]) -> String {
    pages
        .iter()
        .enumerate()
        .map(|(index, page)| format!("<!-- page:{} -->\n{}", index + 1, page.trim()))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

pub async fn extract_pdf(path: &Path, cache_root: &Path, sha256: &str) -> Result<ExtractedPaper> {
    let cache_dir = cache_root.join("extraction").join(sha256);
    let cache_path = cache_dir.join("pages.json");
    if let Ok(bytes) = tokio::fs::read(&cache_path).await {
        let pages: Vec<String> =
            serde_json::from_slice(&bytes).context("decode cached PDF pages")?;
        return Ok(ExtractedPaper {
            markdown: pages_as_markdown(&pages),
            pages,
            cache_path,
        });
    }
    let source = path.to_path_buf();
    let pages = tokio::task::spawn_blocking(move || pdf_extract::extract_text_by_pages(&source))
        .await
        .context("join PDF extraction task")?
        .context("extract PDF text by page")?;
    tokio::fs::create_dir_all(&cache_dir).await?;
    crate::workspace::atomic_write(&cache_path, &serde_json::to_vec(&pages)?).await?;
    Ok(ExtractedPaper {
        markdown: pages_as_markdown(&pages),
        pages,
        cache_path,
    })
}

//! Best-effort, bounded publisher metadata and inline original figures.
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::Client;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};
use tokio::io::AsyncWriteExt;
use url::Url;

fn allowed(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("arxiv.org")
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.path().starts_with("/html/")
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent("PaperCodex/0.1 (daily research briefing)")
        .timeout(Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 3 && allowed(attempt.url()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()?)
}

async fn download(client: &Client, url: Url, limit: usize) -> Result<Vec<u8>> {
    if !allowed(&url) {
        bail!("unsupported figure source");
    }
    let mut response = client.get(url).send().await?.error_for_status()?;
    if response.status().is_redirection()
        || response.content_length().is_some_and(|n| n > limit as u64)
    {
        bail!("figure source exceeds budget or redirects outside publisher");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > limit {
            bail!("figure source exceeds budget");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn download_figure(client: &Client, url: Url, limit: usize) -> Result<Vec<u8>> {
    match download(client, url.clone(), limit).await {
        Ok(bytes) => Ok(bytes),
        Err(error) => {
            // One bounded retry for transport/server failures, never for size
            // rejection, 4xx or invalid sources. The whole image stage also
            // has a hard deadline in BriefingService.
            let transient = error.downcast_ref::<reqwest::Error>().is_some_and(|error| {
                error.is_timeout()
                    || error.is_connect()
                    || error
                        .status()
                        .is_some_and(|status| status.is_server_error())
            });
            if !transient {
                return Err(error);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
            download(client, url, limit).await
        }
    }
}

pub(crate) async fn metadata(paper: &Value, cache: &Path) -> Result<Value> {
    // Prefer the versioned PDF URL, so figures match the edition being read.
    let versioned = paper["metadata"]["links"].as_array().and_then(|links| {
        links
            .iter()
            .filter_map(|link| link["href"].as_str())
            .find(|href| href.contains("arxiv.org/abs/"))
    });
    let versioned =
        versioned.map(|url| url.replace("http://", "https://").replace("/abs/", "/pdf/"));
    let pdf = versioned
        .as_deref()
        .or_else(|| paper["pdf_url"].as_str())
        .context("no arXiv PDF")?;
    let pdf = Url::parse(pdf)?;
    if pdf.host_str() != Some("arxiv.org") {
        bail!("not an arXiv paper");
    }
    let id = pdf
        .path()
        .strip_prefix("/pdf/")
        .context("not an arXiv PDF")?
        .trim_end_matches(".pdf");
    if id.is_empty()
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"./-".contains(&c))
        || id.contains("..")
    {
        bail!("invalid arXiv identifier");
    }
    let url = Url::parse(&format!("https://arxiv.org/html/{id}"))?;
    let key = hex::encode(Sha256::digest(format!(
        "{}:{}",
        url, paper["metadata"]["updated"]
    )));
    let path = cache.join(format!("{key}.json"));
    if let Ok(bytes) = tokio::fs::read(&path).await {
        return Ok(serde_json::from_slice(&bytes)?);
    }
    let bytes = download(&client()?, url.clone(), 2 * 1024 * 1024).await?;
    let mut child = tokio::process::Command::new("python3")
        .arg("-c")
        .arg(include_str!("../scripts/briefing-html-metadata.py"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    child
        .stdin
        .take()
        .context("missing parser stdin")?
        .write_all(&bytes)
        .await?;
    let output = tokio::time::timeout(Duration::from_secs(5), child.wait_with_output()).await??;
    if !output.status.success() {
        bail!("publisher HTML unavailable");
    }
    let mut result: Value = serde_json::from_slice(&output.stdout)?;
    result["source_url"] = json!(url.as_str());
    if let Some(reference) = result["figure"]["image_ref"].as_str() {
        let image_url = url.join(reference)?;
        // Same paper directory only, no external trackers or cross-paper art.
        let image_id = image_url
            .path()
            .strip_prefix("/html/")
            .and_then(|path| path.rsplit_once('/').map(|(id, _)| id));
        let same_paper = image_url.path().starts_with(&format!("/html/{id}/"))
            || image_id.is_some_and(|image_id| {
                image_id == id
                    || (!id.contains('v')
                        && crate::research::canonical_arxiv_id(image_id)
                            == crate::research::canonical_arxiv_id(id))
            });
        if allowed(&image_url) && same_paper && image_url.query().is_none() {
            result["figure"]["image_url"] = json!(image_url.as_str());
        } else {
            result["figure"] = Value::Null;
        }
    }
    tokio::fs::create_dir_all(cache).await?;
    crate::workspace::atomic_write(&path, &serde_json::to_vec(&result)?).await?;
    Ok(result)
}

pub(crate) async fn attach_figures(markdown: &str, sources: &mut [Value]) {
    let Ok(client) = client() else {
        return;
    };
    let mut budget = 5 * 1024 * 1024;
    for source in sources {
        let Some(paper_url) = source["paper"]["source_url"].as_str() else {
            continue;
        };
        if !markdown.contains(&format!("]({paper_url})")) {
            continue;
        }
        let Some(url) = source["presentation"]["figure"]["image_url"]
            .as_str()
            .and_then(|value| Url::parse(value).ok())
        else {
            continue;
        };
        if budget == 0 {
            break;
        }
        let Ok(bytes) = download_figure(&client, url, budget.min(2 * 1024 * 1024)).await else {
            source["presentation"]["figure"]["acquisition_status"] =
                json!("unavailable_or_over_budget");
            continue;
        };
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if bytes.starts_with(b"\xff\xd8\xff") {
            "image/jpeg"
        } else {
            continue;
        };
        budget -= bytes.len();
        let cid = format!("figure-{}@paper-codex", hex::encode(Sha256::digest(&bytes)));
        source["presentation"]["figure"]["mime"] = json!(mime);
        source["presentation"]["figure"]["cid"] = json!(cid);
        source["presentation"]["figure"]["data_base64"] = json!(STANDARD.encode(bytes));
        source["presentation"]["figure"]["acquisition_status"] = json!("acquired");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_fetches_publisher_html_assets() {
        assert!(allowed(
            &Url::parse("https://arxiv.org/html/2609.29204v1/teaser.png").unwrap()
        ));
        for value in [
            "http://arxiv.org/html/x",
            "https://arxiv.org.evil.test/html/x",
            "https://127.0.0.1/html/x",
            "https://user@arxiv.org/html/x",
            "https://arxiv.org:8443/html/x",
            "https://arxiv.org/pdf/x",
        ] {
            assert!(!allowed(&Url::parse(value).unwrap()), "{value}");
        }
    }
}

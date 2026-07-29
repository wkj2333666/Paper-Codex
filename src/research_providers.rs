use crate::research::{
    canonical_arxiv_id, canonical_doi, canonical_key, canonical_openalex_id, EvidenceLevel,
    ResearchProvider, ResearchQuery, WorkMetadata,
};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use reqwest::{header::RETRY_AFTER, Client, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::HashMap, time::Duration};
use url::Url;

const USER_AGENT: &str = "PaperCodex/0.1 (research workspace)";

pub fn research_http_client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()?)
}

#[derive(Clone)]
pub struct OpenAlexProvider {
    client: Client,
    base_url: Url,
}

impl OpenAlexProvider {
    pub fn new(client: Client, base_url: Url) -> Self {
        Self { client, base_url }
    }
}

#[async_trait]
impl ResearchProvider for OpenAlexProvider {
    fn name(&self) -> &'static str {
        "openalex"
    }

    async fn search(&self, query: &ResearchQuery) -> Result<Vec<WorkMetadata>> {
        let query = query.normalized()?;
        let mut url = self.base_url.join("works")?;
        {
            let mut parameters = url.query_pairs_mut();
            parameters.append_pair("search", &query.text);
            parameters.append_pair("per-page", &query.limit.to_string());
            if let Some(filter) = openalex_filter(&query) {
                parameters.append_pair("filter", &filter);
            }
        }
        let body = request_text(&self.client, url).await?;
        parse_openalex_search(&body)
    }
}

#[derive(Clone)]
pub struct CrossrefProvider {
    client: Client,
    base_url: Url,
}

impl CrossrefProvider {
    pub fn new(client: Client, base_url: Url) -> Self {
        Self { client, base_url }
    }
}

#[async_trait]
impl ResearchProvider for CrossrefProvider {
    fn name(&self) -> &'static str {
        "crossref"
    }

    async fn search(&self, query: &ResearchQuery) -> Result<Vec<WorkMetadata>> {
        let query = query.normalized()?;
        let mut url = self.base_url.join("works")?;
        {
            let mut parameters = url.query_pairs_mut();
            parameters.append_pair("query.bibliographic", &query.text);
            parameters.append_pair("rows", &query.limit.to_string());
            if let Some(author) = &query.author {
                parameters.append_pair("query.author", author);
            }
            if let Some(filter) = crossref_filter(&query) {
                parameters.append_pair("filter", &filter);
            }
        }
        let body = request_text(&self.client, url).await?;
        parse_crossref_search(&body)
    }
}

#[derive(Clone)]
pub struct ArxivProvider {
    client: Client,
    base_url: Url,
}

impl ArxivProvider {
    pub fn new(client: Client, base_url: Url) -> Self {
        Self { client, base_url }
    }
}

#[async_trait]
impl ResearchProvider for ArxivProvider {
    fn name(&self) -> &'static str {
        "arxiv"
    }

    async fn search(&self, query: &ResearchQuery) -> Result<Vec<WorkMetadata>> {
        let query = query.normalized()?;
        let mut url = self.base_url.join("api/query")?;
        let search_query = arxiv_query(&query);
        {
            let mut parameters = url.query_pairs_mut();
            parameters.append_pair("search_query", &search_query);
            parameters.append_pair("start", "0");
            parameters.append_pair("max_results", &query.limit.to_string());
            parameters.append_pair("sortBy", "relevance");
        }
        let body = request_text(&self.client, url).await?;
        parse_arxiv_search(&body)
    }
}

pub fn parse_openalex_search(body: &str) -> Result<Vec<WorkMetadata>> {
    let response: OpenAlexResponse =
        serde_json::from_str(body).context("parse OpenAlex search response")?;
    response
        .results
        .into_iter()
        .map(|raw| {
            let item: OpenAlexWork =
                serde_json::from_value(raw.clone()).context("parse OpenAlex work")?;
            let doi = item.doi.as_deref().and_then(canonical_doi);
            let arxiv_id = item
                .ids
                .as_ref()
                .and_then(|ids| ids.arxiv.as_deref())
                .and_then(canonical_arxiv_id);
            let openalex_id = canonical_openalex_id(&item.id)
                .context("OpenAlex work is missing an identifier")?;
            let canonical_key =
                canonical_key(doi.as_deref(), arxiv_id.as_deref(), Some(&openalex_id))
                    .context("OpenAlex work has no canonical identity")?;
            let abstract_text = item
                .abstract_inverted_index
                .as_ref()
                .and_then(rebuild_inverted_abstract);
            let evidence_level = if abstract_text.is_some() {
                EvidenceLevel::Abstract
            } else {
                EvidenceLevel::Metadata
            };
            let source_url = item
                .primary_location
                .as_ref()
                .and_then(|location| location.landing_page_url.clone())
                .or_else(|| doi.as_ref().map(|doi| format!("https://doi.org/{doi}")))
                .unwrap_or_else(|| format!("https://openalex.org/{openalex_id}"));
            let authors = item
                .authorships
                .into_iter()
                .filter_map(|authorship| authorship.author.display_name)
                .filter(|author| !author.trim().is_empty())
                .collect();
            Ok(WorkMetadata {
                canonical_key,
                doi,
                arxiv_id,
                openalex_id: Some(openalex_id),
                title: collapse_whitespace(&item.title),
                authors,
                year: item.publication_year,
                abstract_text,
                source_url,
                pdf_url: item.best_oa_location.and_then(|location| location.pdf_url),
                evidence_level,
                metadata: raw,
            })
        })
        .collect()
}

pub fn parse_crossref_search(body: &str) -> Result<Vec<WorkMetadata>> {
    let response: CrossrefResponse =
        serde_json::from_str(body).context("parse Crossref search response")?;
    response
        .message
        .items
        .into_iter()
        .map(|raw| {
            let item: CrossrefWork =
                serde_json::from_value(raw.clone()).context("parse Crossref work")?;
            let doi = canonical_doi(&item.doi).context("Crossref work is missing a valid DOI")?;
            let canonical_key = format!("doi:{doi}");
            let abstract_text = item
                .abstract_text
                .as_deref()
                .map(strip_markup)
                .filter(|abstract_text| !abstract_text.is_empty());
            let evidence_level = if abstract_text.is_some() {
                EvidenceLevel::Abstract
            } else {
                EvidenceLevel::Metadata
            };
            let pdf_url = item.links.into_iter().find_map(|link| {
                link.content_type
                    .as_deref()
                    .is_some_and(|content_type| {
                        content_type.eq_ignore_ascii_case("application/pdf")
                    })
                    .then_some(link.url)
            });
            let authors = item
                .authors
                .into_iter()
                .map(|author| {
                    collapse_whitespace(&format!(
                        "{} {}",
                        author.given.unwrap_or_default(),
                        author.family.unwrap_or_default()
                    ))
                })
                .filter(|author| !author.is_empty())
                .collect();
            let year = item
                .published
                .as_ref()
                .or(item.issued.as_ref())
                .and_then(CrossrefDate::year);
            Ok(WorkMetadata {
                canonical_key,
                doi: Some(doi.clone()),
                arxiv_id: None,
                openalex_id: None,
                title: item
                    .titles
                    .first()
                    .map(|title| collapse_whitespace(title))
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| "Untitled work".to_owned()),
                authors,
                year,
                abstract_text,
                source_url: item.url.unwrap_or_else(|| format!("https://doi.org/{doi}")),
                pdf_url,
                evidence_level,
                metadata: raw,
            })
        })
        .collect()
}

pub fn parse_arxiv_search(body: &str) -> Result<Vec<WorkMetadata>> {
    let response: ArxivFeed =
        quick_xml::de::from_str(body).context("parse arXiv search response")?;
    response
        .entries
        .into_iter()
        .map(|item| {
            let arxiv_id =
                canonical_arxiv_id(&item.id).context("arXiv entry has an invalid identifier")?;
            let year = item
                .published
                .as_deref()
                .and_then(|published| published.get(..4))
                .and_then(|year| year.parse().ok());
            let abstract_text = {
                let summary = collapse_whitespace(&item.summary);
                (!summary.is_empty()).then_some(summary)
            };
            let metadata = json!({
                "provider": "arxiv",
                "updated": item.updated,
                "links": item.links,
            });
            Ok(WorkMetadata {
                canonical_key: format!("arxiv:{arxiv_id}"),
                doi: None,
                arxiv_id: Some(arxiv_id.clone()),
                openalex_id: None,
                title: collapse_whitespace(&item.title),
                authors: item
                    .authors
                    .into_iter()
                    .map(|author| collapse_whitespace(&author.name))
                    .filter(|author| !author.is_empty())
                    .collect(),
                year,
                abstract_text: abstract_text.clone(),
                source_url: format!("https://arxiv.org/abs/{arxiv_id}"),
                pdf_url: Some(format!("https://arxiv.org/pdf/{arxiv_id}.pdf")),
                evidence_level: if abstract_text.is_some() {
                    EvidenceLevel::Abstract
                } else {
                    EvidenceLevel::Metadata
                },
                metadata,
            })
        })
        .collect()
}

async fn request_text(client: &Client, url: Url) -> Result<String> {
    for attempt in 0..=1 {
        let response = client
            .get(url.clone())
            .send()
            .await
            .with_context(|| format!("send provider request to {url}"))?;
        let status = response.status();
        if attempt == 0 && (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()) {
            let delay = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(1)
                .min(5);
            tokio::time::sleep(Duration::from_secs(delay)).await;
            continue;
        }
        let body = response.text().await.context("read provider response")?;
        if !status.is_success() {
            bail!(
                "provider request returned HTTP {status}: {}",
                body.chars().take(200).collect::<String>()
            );
        }
        return Ok(body);
    }
    unreachable!("provider retry loop always returns on its final attempt")
}

fn rebuild_inverted_abstract(index: &HashMap<String, Vec<usize>>) -> Option<String> {
    let length = index
        .values()
        .flat_map(|positions| positions.iter())
        .max()
        .map(|position| position + 1)?;
    let mut words = vec![None; length];
    for (word, positions) in index {
        for &position in positions {
            if let Some(slot) = words.get_mut(position) {
                *slot = Some(word.as_str());
            }
        }
    }
    let abstract_text = words
        .into_iter()
        .map(|word| word.unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ");
    let abstract_text = collapse_whitespace(&abstract_text);
    (!abstract_text.is_empty()).then_some(abstract_text)
}

fn strip_markup(value: &str) -> String {
    let mut text = String::with_capacity(value.len());
    let mut inside_tag = false;
    for character in value.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => {
                inside_tag = false;
                text.push(' ');
            }
            _ if !inside_tag => text.push(character),
            _ => {}
        }
    }
    collapse_whitespace(
        &text
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">"),
    )
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn openalex_filter(query: &ResearchQuery) -> Option<String> {
    let mut filters = Vec::new();
    if let Some(year) = query.year_from {
        filters.push(format!("from_publication_date:{year}-01-01"));
    }
    if let Some(year) = query.year_to {
        filters.push(format!("to_publication_date:{year}-12-31"));
    }
    (!filters.is_empty()).then(|| filters.join(","))
}

fn crossref_filter(query: &ResearchQuery) -> Option<String> {
    let mut filters = Vec::new();
    if let Some(year) = query.year_from {
        filters.push(format!("from-pub-date:{year}-01-01"));
    }
    if let Some(year) = query.year_to {
        filters.push(format!("until-pub-date:{year}-12-31"));
    }
    (!filters.is_empty()).then(|| filters.join(","))
}

fn arxiv_query(query: &ResearchQuery) -> String {
    let mut clauses = vec![format!("all:\"{}\"", query.text.replace('"', ""))];
    for title in &query.title_terms {
        clauses.push(format!("ti:\"{}\"", title.replace('"', "")));
    }
    if let Some(author) = &query.author {
        clauses.push(format!("au:\"{}\"", author.replace('"', "")));
    }
    clauses.join(" AND ")
}

#[derive(Deserialize)]
struct OpenAlexResponse {
    results: Vec<Value>,
}

#[derive(Deserialize)]
struct OpenAlexWork {
    id: String,
    doi: Option<String>,
    title: String,
    publication_year: Option<i64>,
    #[serde(default)]
    authorships: Vec<OpenAlexAuthorship>,
    abstract_inverted_index: Option<HashMap<String, Vec<usize>>>,
    primary_location: Option<OpenAlexLocation>,
    best_oa_location: Option<OpenAlexLocation>,
    ids: Option<OpenAlexIds>,
}

#[derive(Deserialize)]
struct OpenAlexAuthorship {
    author: OpenAlexAuthor,
}

#[derive(Deserialize)]
struct OpenAlexAuthor {
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct OpenAlexLocation {
    landing_page_url: Option<String>,
    pdf_url: Option<String>,
}

#[derive(Deserialize)]
struct OpenAlexIds {
    arxiv: Option<String>,
}

#[derive(Deserialize)]
struct CrossrefResponse {
    message: CrossrefMessage,
}

#[derive(Deserialize)]
struct CrossrefMessage {
    items: Vec<Value>,
}

#[derive(Deserialize)]
struct CrossrefWork {
    #[serde(rename = "DOI")]
    doi: String,
    #[serde(rename = "title", default)]
    titles: Vec<String>,
    #[serde(rename = "author", default)]
    authors: Vec<CrossrefAuthor>,
    published: Option<CrossrefDate>,
    issued: Option<CrossrefDate>,
    #[serde(rename = "abstract")]
    abstract_text: Option<String>,
    #[serde(rename = "URL")]
    url: Option<String>,
    #[serde(rename = "link", default)]
    links: Vec<CrossrefLink>,
}

#[derive(Deserialize)]
struct CrossrefAuthor {
    given: Option<String>,
    family: Option<String>,
}

#[derive(Deserialize)]
struct CrossrefDate {
    #[serde(rename = "date-parts")]
    date_parts: Vec<Vec<i64>>,
}

impl CrossrefDate {
    fn year(&self) -> Option<i64> {
        self.date_parts
            .first()
            .and_then(|parts| parts.first())
            .copied()
    }
}

#[derive(Deserialize)]
struct CrossrefLink {
    #[serde(rename = "URL")]
    url: String,
    #[serde(rename = "content-type")]
    content_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ArxivFeed {
    #[serde(rename = "entry", default)]
    entries: Vec<ArxivEntry>,
}

#[derive(Debug, Deserialize)]
struct ArxivEntry {
    id: String,
    title: String,
    summary: String,
    published: Option<String>,
    updated: Option<String>,
    #[serde(rename = "author", default)]
    authors: Vec<ArxivAuthor>,
    #[serde(rename = "link", default)]
    links: Vec<ArxivLink>,
}

#[derive(Debug, Deserialize)]
struct ArxivAuthor {
    name: String,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct ArxivLink {
    #[serde(rename = "@href")]
    href: String,
    #[serde(rename = "@rel")]
    rel: Option<String>,
    #[serde(rename = "@type")]
    content_type: Option<String>,
}

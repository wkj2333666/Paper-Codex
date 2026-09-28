//! Auditable retrieval; incomplete feeds must never become a successful empty day.
use crate::research::WorkMetadata;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

pub(crate) fn since(previous: Option<&str>, now: DateTime<Utc>) -> DateTime<Utc> {
    let previous = previous
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or(now)
        .min(now);
    (previous - Duration::days(7)).max(now - Duration::days(30))
}

pub(crate) fn total_results(body: &str) -> Result<Option<usize>> {
    #[derive(Deserialize)]
    struct Envelope {
        #[serde(rename = "totalResults", default)]
        total: Option<usize>,
    }
    Ok(quick_xml::de::from_str::<Envelope>(body)
        .context("读取 arXiv 结果总数失败")?
        .total)
}

#[derive(Default, Serialize)]
pub(crate) struct FetchAudit {
    pub query: String,
    pub pages: usize,
    pub total_results: Option<usize>,
    pub received: usize,
    pub within_window: usize,
    pub before_window: usize,
    pub latest_updated: Option<String>,
    pub oldest_updated: Option<String>,
    pub complete: bool,
    pub limit_reached: bool,
    #[serde(skip)]
    last_date: Option<DateTime<Utc>>,
}
impl FetchAudit {
    pub fn new(query: &str) -> Self {
        Self {
            query: query.into(),
            ..Default::default()
        }
    }
    pub fn page(
        &mut self,
        entries: Vec<WorkMetadata>,
        total: Option<usize>,
        since: DateTime<Utc>,
    ) -> Result<Vec<WorkMetadata>> {
        self.pages += 1;
        self.total_results = total;
        if entries.is_empty()
            && (total.is_none() || total.is_some_and(|count| count > self.received))
        {
            bail!("arXiv 返回不完整的空结果页，不能判定为无新增论文");
        }
        if total == Some(0) && !entries.is_empty() {
            bail!("arXiv 总数与结果页不一致");
        }
        let count = entries.len();
        self.received += count;
        let mut recent = Vec::new();
        for paper in entries {
            let updated = paper.metadata["updated"]
                .as_str()
                .context("arXiv 返回缺少更新时间")?;
            let date = DateTime::parse_from_rfc3339(updated)?.with_timezone(&Utc);
            if self.last_date.is_some_and(|previous| date > previous) {
                bail!("arXiv 结果未按更新时间排序，停止推进检索位置");
            }
            self.last_date = Some(date);
            if self.latest_updated.is_none() {
                self.latest_updated = Some(updated.to_owned());
            }
            self.oldest_updated = Some(updated.to_owned());
            if date < since {
                self.before_window += 1;
            } else {
                self.within_window += 1;
                recent.push(paper);
            }
        }
        self.complete = self.before_window > 0
            || total
                .map(|count| self.received >= count)
                .unwrap_or(count < 200);
        Ok(recent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_prefixed_total_and_rejects_false_empty_pages() {
        assert_eq!(total_results(r#"<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opensearch="http://a9.com/-/spec/opensearch/1.1/"><opensearch:totalResults>42</opensearch:totalResults></feed>"#).unwrap(), Some(42));
        let mut audit = FetchAudit::new("query");
        assert!(audit.page(vec![], Some(42), Utc::now()).is_err());
        assert!(!audit.complete);
        let mut audit = FetchAudit::new("query");
        assert!(audit.page(vec![], None, Utc::now()).is_err());
        let mut audit = FetchAudit::new("query");
        assert!(audit.page(vec![], Some(0), Utc::now()).unwrap().is_empty());
        assert!(audit.complete);
    }
    #[test]
    fn overlap_covers_weekends_and_delayed_indexing_but_is_bounded() {
        let now = DateTime::parse_from_rfc3339("2026-09-28T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            since(Some("2026-09-27T08:49:38Z"), now).to_rfc3339(),
            "2026-09-20T08:49:38+00:00"
        );
        assert_eq!(since(None, now), now - Duration::days(7));
        assert_eq!(
            since(Some("2020-01-01T00:00:00Z"), now),
            now - Duration::days(30)
        );
    }

    #[test]
    fn counts_date_filter_and_does_not_finish_a_short_partial_page() {
        let mut works = crate::research_providers::parse_arxiv_search(include_str!(
            "../fixtures/research/arxiv-search.xml"
        ))
        .unwrap();
        works[0].metadata["updated"] = serde_json::json!("2026-09-25T14:21:20Z");
        works[1].metadata["updated"] = serde_json::json!("2026-09-24T00:00:00Z");
        let since = DateTime::parse_from_rfc3339("2026-09-25T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut audit = FetchAudit::new("query");
        assert_eq!(
            audit
                .page(vec![works[0].clone()], Some(200), since)
                .unwrap()
                .len(),
            1
        );
        assert!(!audit.complete);
        assert_eq!(audit.received, 1);
        assert_eq!(
            audit
                .page(vec![works[1].clone()], Some(200), since)
                .unwrap()
                .len(),
            0
        );
        assert!(audit.complete);
        assert_eq!(audit.within_window, 1);
        assert_eq!(audit.before_window, 1);
        let mut audit = FetchAudit::new("query");
        assert!(audit
            .page(vec![works[1].clone(), works[0].clone()], Some(2), since)
            .is_err());
        assert!(!audit.complete);
    }
}

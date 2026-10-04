//! Select useful evidence for a readable digest with complete paper introductions.
use crate::research::WorkMetadata;
use regex::Regex;
use serde_json::Value;
use std::{collections::BTreeSet, sync::OnceLock};

fn normalized_terms(value: &str) -> String {
    value
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn comparable_word(word: &str) -> String {
    if let Some(stem) = word.strip_suffix("ies") {
        return format!("{stem}y");
    }
    if word.len() > 4 {
        word.strip_suffix('s').unwrap_or(word).to_owned()
    } else {
        word.to_owned()
    }
}

fn contains_topic(text: &str, term: &str) -> bool {
    let tokens = crate::briefing_project::search_words(term);
    if tokens.is_empty() {
        return false;
    }
    let words: BTreeSet<_> = text.split_whitespace().map(comparable_word).collect();
    tokens
        .iter()
        .all(|token| words.contains(&comparable_word(token)))
}

pub(crate) fn topic_score(paper: &WorkMetadata, keywords: &[String]) -> usize {
    let title = normalized_terms(&paper.title);
    let text = format!(
        "{} {}",
        title,
        normalized_terms(paper.abstract_text.as_deref().unwrap_or_default())
    );
    keywords
        .iter()
        .map(|keyword| {
            let word = normalized_terms(keyword);
            let specificity = if crate::briefing_project::search_words(&word).len() <= 1 {
                1
            } else {
                4
            };
            specificity
                * (usize::from(contains_topic(&title, &word)) * 3
                    + usize::from(contains_topic(&text, &word)))
        })
        .max()
        .unwrap_or(0)
}

/// Reuse the briefing's semantic search plan; never hard-code folder names or
/// confuse an incidental abstract match with a strong title match. Suggestions
/// are advisory and revalidated against the current project tree on every read.
pub(crate) fn project_suggestions(
    paper: &WorkMetadata,
    plan: &Value,
    sources: &[Value],
    owner: &str,
    projects: &[crate::domain::Project],
) -> Vec<Value> {
    let mut scores = std::collections::BTreeMap::<String, (usize, String)>::new();
    let topics = plan["topics"].as_array().cloned().unwrap_or_else(|| {
        sources
            .iter()
            .find(|source| source["paper"]["canonical_key"] == paper.canonical_key)
            .and_then(|source| source["research_topics"].as_array())
            .cloned()
            .unwrap_or_default()
    });
    for topic in topics {
        let terms: Vec<String> = topic["terms"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        let score = if terms.is_empty() {
            1
        } else {
            topic_score(paper, &terms)
        };
        if score == 0 {
            continue;
        }
        for id in topic["project_ids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let mut cursor = Some(id);
            let mut visited = BTreeSet::new();
            let mut in_scope = false;
            while let Some(current) = cursor {
                if !visited.insert(current) {
                    break;
                }
                let Some(project) = projects.iter().find(|p| p.id == current) else {
                    break;
                };
                if current == owner {
                    in_scope = true;
                    break;
                }
                cursor = project.parent_id.as_deref();
            }
            if !in_scope {
                continue;
            }
            let entry = scores.entry(id.to_owned()).or_default();
            if score > entry.0 {
                *entry = (
                    score,
                    topic["label"].as_str().unwrap_or("晨报研究主题").to_owned(),
                );
            }
        }
    }
    // Prefer a supported descendant over its generic ancestor on equal evidence.
    let ids: BTreeSet<_> = scores.keys().cloned().collect();
    let mut ranked: Vec<_> = scores
        .iter()
        .filter(|(id, (score, _))| {
            !ids.iter().any(|other| {
                if other == *id || scores[other].0 < *score {
                    return false;
                }
                let mut cursor = projects
                    .iter()
                    .find(|p| &p.id == other)
                    .and_then(|p| p.parent_id.as_deref());
                let mut seen = BTreeSet::new();
                while let Some(parent) = cursor {
                    if !seen.insert(parent) {
                        break;
                    }
                    if parent == id.as_str() {
                        return true;
                    }
                    cursor = projects
                        .iter()
                        .find(|p| p.id == parent)
                        .and_then(|p| p.parent_id.as_deref());
                }
                false
            })
        })
        .collect();
    ranked.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(b.0)));
    ranked.into_iter().map(|(id, (score, reason))| serde_json::json!({"project_id":id,"score":score,"reason":reason})).collect()
}

pub(crate) fn candidates<'a>(
    papers: &'a [WorkMetadata],
    keywords: &[String],
    limit: usize,
) -> Vec<&'a WorkMetadata> {
    let mut ranked: Vec<_> = papers
        .iter()
        .filter_map(|paper| {
            let score = topic_score(paper, keywords);
            (score > 0).then_some((score, paper))
        })
        .collect();
    // This is relevance triage, not a claim about scientific importance.
    ranked.sort_by(|(a_score, a), (b_score, b)| {
        b_score
            .cmp(a_score)
            .then_with(|| {
                b.metadata["updated"]
                    .as_str()
                    .cmp(&a.metadata["updated"].as_str())
            })
            .then_with(|| a.canonical_key.cmp(&b.canonical_key))
    });
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, paper)| paper)
        .collect()
}

pub(crate) fn select<'a>(
    papers: &'a [WorkMetadata],
    plan: &crate::briefing_project::SearchPlan,
    limit: usize,
) -> Vec<&'a WorkMetadata> {
    let ranked: Vec<_> = plan
        .topics
        .iter()
        .map(|topic| candidates(papers, &topic.terms, papers.len()))
        .collect();
    let mut order: Vec<_> = (0..plan.topics.len()).collect();
    order.sort_by_key(|index| (std::cmp::Reverse(plan.topics[*index].priority), *index));
    let mut chosen: Vec<&WorkMetadata> = Vec::new();
    // Cover semantic questions, not folder counts. A strong cross-cutting paper
    // can represent more than one topic without being repeated.
    for index in order {
        if chosen.len() >= limit {
            break;
        }
        let Some(best) = ranked[index].first() else {
            continue;
        };
        let topic = &plan.topics[index];
        let best_score = topic_score(best, &topic.terms);
        if chosen
            .iter()
            .any(|paper| topic_score(paper, &topic.terms) >= best_score)
        {
            continue;
        }
        if let Some(paper) = ranked[index].iter().find(|paper| {
            !chosen
                .iter()
                .any(|old| old.canonical_key == paper.canonical_key)
        }) {
            chosen.push(paper);
        }
    }
    let mut remaining: Vec<_> = papers
        .iter()
        .filter_map(|paper| {
            let score = plan
                .topics
                .iter()
                .zip(&ranked)
                .map(|(topic, ranking)| {
                    let best = ranking
                        .first()
                        .map_or(1, |best| topic_score(best, &topic.terms));
                    topic_score(paper, &topic.terms) * usize::from(topic.priority) * 100
                        / best.max(1)
                })
                .max()
                .unwrap_or(0);
            (score > 0).then_some((score, paper))
        })
        .collect();
    remaining.sort_by(|(a_score, a), (b_score, b)| {
        b_score
            .cmp(a_score)
            .then_with(|| {
                b.metadata["updated"]
                    .as_str()
                    .cmp(&a.metadata["updated"].as_str())
            })
            .then_with(|| a.canonical_key.cmp(&b.canonical_key))
    });
    for (_, paper) in remaining {
        if chosen.len() >= limit {
            break;
        }
        if !chosen
            .iter()
            .any(|old| old.canonical_key == paper.canonical_key)
        {
            chosen.push(paper);
        }
    }
    chosen
}

pub(crate) fn coverage(
    papers: &[WorkMetadata],
    chosen: &[&WorkMetadata],
    plan: &crate::briefing_project::SearchPlan,
) -> Value {
    serde_json::json!(plan.topics.iter().map(|topic| {
        let candidates = papers.iter().filter(|paper| topic_score(paper, &topic.terms)>0).count();
        let selected = chosen.iter().filter(|paper| topic_score(paper, &topic.terms)>0).count();
        serde_json::json!({"id":topic.id,"label":topic.label,"project_ids":topic.project_ids,"intent":topic.intent,"candidates":candidates,"selected":selected,"status":if candidates==0 {"no_unseen_candidates"} else if selected==0 {"budget_limited"} else {"selected"}})
    }).collect::<Vec<_>>())
}

fn section_kind(line: &str) -> Option<usize> {
    static HEADING: OnceLock<Regex> = OnceLock::new();
    let line = line.trim();
    if line.len() > 140 || line.contains("....") {
        return None;
    }
    let pattern = HEADING.get_or_init(|| Regex::new(r"(?i)^(?:#{1,6}\s*)?(?:(?:\d+(?:\.\d+)*|[ivx]+)[.)]?\s+)?(methods?|methodology|approach|architecture|experiments?|experimental|results?|evaluation|ablations?|discussion|conclusions?|limitations?|references|bibliography)(?:\s|:|\.|$)").unwrap());
    let name = pattern.captures(line)?.get(1)?.as_str().to_lowercase();
    if matches!(name.as_str(), "references" | "bibliography")
        && !line
            .trim_end_matches(['.', ':'])
            .to_lowercase()
            .ends_with(&name)
    {
        return None;
    }
    Some(match name.as_str() {
        "method" | "methods" | "methodology" | "approach" | "architecture" => 0,
        "discussion" | "conclusion" | "conclusions" | "limitation" | "limitations" => 2,
        "references" | "bibliography" => 3,
        _ => 1,
    })
}

fn end_after(text: &str, start: usize, characters: usize) -> usize {
    text[start..]
        .char_indices()
        .nth(characters)
        .map_or(text.len(), |(offset, _)| start + offset)
}

pub(crate) fn evidence_excerpt(text: &str) -> (String, bool) {
    if text.chars().count() <= 22000 {
        return (text.to_owned(), false);
    }
    let head_end = end_after(text, 0, 5000);
    let mut offset = 0;
    let headings: Vec<_> = text
        .split_inclusive('\n')
        .filter_map(|line| {
            let start = offset;
            offset += line.len();
            section_kind(line).map(|kind| (start, kind))
        })
        .collect();
    let content_end = headings
        .iter()
        .find(|(start, kind)| *kind == 3 && *start > head_end)
        .map_or(text.len(), |(start, _)| *start);
    let content = &text[..content_end];
    let mut ranges = vec![(0, head_end.min(content_end))];
    let count = content.chars().count();
    for (kind, budget, fallback_percent) in [(0, 4000, 20), (1, 6000, 50), (2, 4000, 80)] {
        let start = headings
            .iter()
            .find(|(start, found)| *found == kind && *start < content_end)
            .map_or_else(
                || end_after(content, 0, count * fallback_percent / 100),
                |(start, _)| *start,
            );
        // Fill missing section cues with positional samples, without labeling
        // those samples as methods/results that the document did not identify.
        ranges.push((start, end_after(content, start, budget)));
    }
    let tail_start = content
        .char_indices()
        .rev()
        .nth(1999)
        .map_or(0, |(start, _)| start);
    ranges.push((tail_start, content_end));
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut().filter(|last| start <= last.1) {
            last.1 = last.1.max(end);
        } else {
            merged.push((start, end));
        }
    }
    let excerpt = merged
        .iter()
        .map(|(start, end)| &text[*start..*end])
        .collect::<Vec<_>>()
        .join("\n\n[中间原文已省略；以下为另一处节选]\n\n");
    (excerpt, true)
}

pub(crate) fn prompt(
    day: &str,
    since: &str,
    projects: &Value,
    interests: &Value,
    sources: &[Value],
) -> String {
    let policy = projects["briefing_editorial_skill"]
        .as_str()
        .unwrap_or(crate::briefing_skill::EDITORIAL);
    let mut project_context = projects.clone();
    if let Some(object) = project_context.as_object_mut() {
        object.remove("briefing_editorial_skill");
    }
    let budget = serde_json::json!({
        "candidate_count": sources.len(),
        "deep_reading_suggestions": sources.len().min(3),
        "quick_reading_suggestions": sources.len().saturating_sub(3).min(6),
        "note": "篇数是阅读预算参考，不是分支配额。主题覆盖和完整工作介绍优先；重要主题不能因篇幅被无声省略。"
    });
    format!(
        "{policy}\n\n当前模式：晨报撰写。日期 {day}（北京时间），检索窗口起点 {since}。\n阅读预算：{budget}\n项目与覆盖记录：{project_context}\n已保存的阅读偏好：{interests}\n实际证据：{sources}\n只返回读者正文，不调用工具或发送邮件。",
        sources = serde_json::to_string(sources).expect("JSON values serialize")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::EvidenceLevel;
    use serde_json::json;

    fn paper(id: &str, title: &str, summary: &str, updated: &str) -> WorkMetadata {
        WorkMetadata {
            canonical_key: id.into(),
            title: title.into(),
            abstract_text: Some(summary.into()),
            metadata: json!({"updated":updated}),
            doi: None,
            arxiv_id: None,
            openalex_id: None,
            authors: vec![],
            year: None,
            source_url: format!("https://arxiv.org/abs/{id}"),
            pdf_url: None,
            evidence_level: EvidenceLevel::Abstract,
        }
    }

    #[test]
    fn prioritizes_specific_interest_and_title_over_generic_keyword_volume() {
        let papers = vec![
            paper(
                "generic",
                "Robot learning",
                "robot embodied manipulation humanoid",
                "2026-09-27",
            ),
            paper(
                "focused",
                "Long-horizon failure recovery",
                "closed-loop planning",
                "2026-09-26",
            ),
            paper("unrelated", "Music", "audio", "2026-09-27"),
        ];
        let words = [
            "robot",
            "embodied",
            "manipulation",
            "humanoid",
            "failure recovery",
            "FAILURE RECOVERY",
        ]
        .map(String::from);
        let chosen = candidates(&papers, &words, 2);
        assert_eq!(
            chosen
                .iter()
                .map(|p| p.canonical_key.as_str())
                .collect::<Vec<_>>(),
            ["focused", "generic"]
        );
        assert!(candidates(&papers, &["unmatched".into()], 12).is_empty());
        assert_eq!(candidates(&papers, &words, 1).len(), 1);
    }

    #[test]
    fn import_suggestions_use_topic_strength_and_live_arbitrary_depth_tree() {
        let project = |id: &str, parent: Option<&str>| crate::domain::Project {
            id: id.into(),
            slug: id.into(),
            name: id.into(),
            purpose: String::new(),
            parent_id: parent.map(str::to_owned),
            sort_order: 0,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let projects = vec![
            project("root", None),
            project("branch", Some("root")),
            project("leaf", Some("branch")),
            project("other", Some("root")),
            project("outside", None),
        ];
        let plan = json!({"topics":[
            {"label":"Main method","project_ids":["branch","leaf","deleted","outside"],"terms":["world action model"]},
            {"label":"Related background","project_ids":["other"],"terms":["robot"]}
        ]});
        let work = paper("work", "World Action Models", "robot", "2026-10-04");
        let result = project_suggestions(&work, &plan, &[], "root", &projects);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["project_id"], "leaf");
        assert_eq!(result[1]["project_id"], "other");
        assert!(result[0]["score"].as_u64() > result[1]["score"].as_u64());
        let unmatched = paper("work", "Unrelated", "music", "2026-10-04");
        assert!(project_suggestions(&unmatched, &plan, &[], "root", &projects).is_empty());
    }

    #[test]
    fn project_phrase_matching_tolerates_hyphens_and_line_breaks() {
        let papers = vec![paper(
            "vla",
            "Vision-Language-Action",
            "world\n  model",
            "2026-09-27",
        )];
        assert_eq!(
            candidates(&papers, &["vision language action".into()], 12).len(),
            1
        );
        assert_eq!(candidates(&papers, &["world model".into()], 12).len(), 1);
    }

    #[test]
    fn search_matches_reordered_topics_and_plural_words_without_loose_substrings() {
        let papers = vec![
            paper(
                "wam",
                "A world-action model",
                "Multiple world models enable robotic manipulation.",
                "2026-09-25",
            ),
            paper(
                "unrelated",
                "Museum modelling",
                "Model railway",
                "2026-09-25",
            ),
        ];
        let chosen = candidates(&papers, &["world model for manipulation".into()], 12);
        assert_eq!(chosen.len(), 1);
        assert_eq!(chosen[0].canonical_key, "wam");
        assert!(candidates(&papers, &["humanoid".into()], 12).is_empty());
    }

    #[test]
    fn long_papers_keep_late_results_and_conclusions_with_explicit_gaps() {
        let text = format!("# Introduction\n{}\n## 2. Method\n{}\n## 3. Results\nLATE_RESULT_42\n{}\n## 4. Conclusion\nBOUNDARY_CONDITION\n{}\n## References\n{}", "前言".repeat(9000), "方法".repeat(6000), "实验".repeat(6000), "讨论".repeat(2000), "引用".repeat(9000));
        let (excerpt, truncated) = evidence_excerpt(&text);
        assert!(truncated);
        assert!(excerpt.contains("LATE_RESULT_42"));
        assert!(excerpt.contains("BOUNDARY_CONDITION"));
        assert!(excerpt.contains("中间原文已省略"));
        assert!(!excerpt.contains("## References"));
        assert!(excerpt.chars().count() <= 22000);
    }

    #[test]
    fn keeps_short_text_exact_and_bounds_unstructured_unicode_text() {
        assert_eq!(
            evidence_excerpt("短文🙂\n结论"),
            ("短文🙂\n结论".into(), false)
        );
        let (excerpt, truncated) =
            evidence_excerpt(&format!("{}TAIL_RESULT", "文本🙂".repeat(18000)));
        assert!(truncated);
        assert!(excerpt.ends_with("TAIL_RESULT"));
        assert!(excerpt.chars().count() <= 22000);
        assert_eq!(section_kind("References to earlier work appear here"), None);
        assert_eq!(section_kind("VII. REFERENCES"), Some(3));
    }

    #[test]
    fn prompt_keeps_runtime_policy_coverage_and_evidence_in_separate_fields() {
        let sources = vec![
            json!({"paper":{"title":"Complete title","authors":["A","B"]},"evidence":"abstract"}),
        ];
        let context = json!({"briefing_editorial_skill":"CUSTOM_POLICY", "briefing_coverage":[{"label":"Shared method across nested folders","selected":1}],"project":{"name":"Project X"}});
        let output = prompt(
            "2026-09-27",
            "2026-09-23",
            &context,
            &json!(["讲清机制"]),
            &sources,
        );
        assert_eq!(output.matches("CUSTOM_POLICY").count(), 1);
        assert!(output.contains("Shared method across nested folders"));
        assert!(output.contains("Complete title"));
        assert!(output.contains(r#""authors":["A","B"]"#));
        assert!(output.contains(r#""candidate_count":1"#));
        assert!(output.contains(r#""quick_reading_suggestions":0"#));
        assert!(output.contains("讲清机制"));
    }

    #[test]
    fn supplies_complete_titles_and_authors_without_synthetic_affiliations() {
        let source = json!({"paper": {
            "title": "Example: A Complete Research Title",
            "authors": ["First Author", "Second Author"],
            "source_url": "https://example.test/paper"
        }, "evidence": "abstract"});
        let output = prompt(
            "2026-09-27",
            "2026-09-23",
            &json!([]),
            &json!([]),
            &[source],
        );
        for expected in [
            "Example: A Complete Research Title",
            "First Author",
            "Second Author",
            "author/affiliation",
            "abstract-only",
        ] {
            assert!(output.contains(expected), "missing {expected}");
        }
        assert!(!output.contains("不铺满英文全名"));
        assert!(!output.contains("150–230"));
    }

    fn themed_plan() -> crate::briefing_project::SearchPlan {
        use crate::briefing_project::{SearchPlan, SearchTopic};
        SearchPlan {
            terms: vec![],
            rationale: "semantic coverage".into(),
            dispositions: vec![],
            topics: vec![
                SearchTopic {
                    id: "actions".into(),
                    label: "动作模型".into(),
                    project_ids: vec!["a".into()],
                    intent: "Action policies".into(),
                    priority: 2,
                    terms: vec![
                        "vision language action".into(),
                        "vision-language-action model".into(),
                        "action token".into(),
                    ],
                },
                SearchTopic {
                    id: "worlds".into(),
                    label: "预测世界".into(),
                    project_ids: vec!["b".into(), "deep-child".into()],
                    intent: "World action dynamics".into(),
                    priority: 2,
                    terms: vec!["world action model".into(), "world model".into()],
                },
                SearchTopic {
                    id: "agents".into(),
                    label: "自主任务".into(),
                    project_ids: vec!["c".into()],
                    intent: "Agent planning".into(),
                    priority: 2,
                    terms: vec!["embodied agent".into()],
                },
            ],
        }
    }

    #[test]
    fn prolific_topic_and_duplicate_aliases_do_not_erase_other_questions() {
        let mut papers: Vec<_> = (0..20)
            .map(|index| {
                paper(
                    &format!("vla-{index}"),
                    "Vision-language-action model with action tokens",
                    "robot control",
                    "2026-09-27",
                )
            })
            .collect();
        papers.push(paper(
            "world",
            "World action model",
            "predicting dynamics",
            "2026-09-25",
        ));
        papers.push(paper(
            "agent",
            "Embodied agent planning",
            "reasoning",
            "2026-09-24",
        ));
        let plan = themed_plan();
        let selected = select(&papers, &plan, 6);
        assert_eq!(selected.len(), 6);
        assert!(selected.iter().any(|paper| paper.canonical_key == "world"));
        assert!(selected.iter().any(|paper| paper.canonical_key == "agent"));
        let mut aliases = plan.clone();
        aliases.topics[0].terms.extend(plan.topics[0].terms.clone());
        assert_eq!(
            select(&papers, &aliases, 6)
                .iter()
                .map(|p| &p.canonical_key)
                .collect::<Vec<_>>(),
            selected
                .iter()
                .map(|p| &p.canonical_key)
                .collect::<Vec<_>>()
        );
        let scarce = select(&papers, &plan, 1);
        let coverage = coverage(&papers, &scarce, &plan);
        assert_eq!(coverage[1]["status"], "budget_limited");
        assert_eq!(coverage[2]["status"], "budget_limited");
    }

    #[test]
    fn cross_cutting_work_is_not_duplicated_and_empty_topics_get_no_fake_quota() {
        let papers = vec![paper(
            "shared",
            "World action model with embodied agent planning",
            "",
            "2026-09-27",
        )];
        let plan = themed_plan();
        let selected = select(&papers, &plan, 12);
        assert_eq!(selected.len(), 1);
        let coverage = coverage(&papers, &selected, &plan);
        assert_eq!(coverage[0]["status"], "no_unseen_candidates");
        assert_eq!(coverage[1]["selected"], 1);
        assert_eq!(coverage[2]["selected"], 1);
    }
}

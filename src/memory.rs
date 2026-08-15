use crate::conversations::MemoryItem;
use chrono::{DateTime, NaiveDateTime, Utc};

const MEMORY_CANDIDATE_MAX_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryCandidate {
    pub scope_type: String,
    pub kind: String,
    pub value: String,
    pub source: String,
    pub confidence: String,
}

pub fn extract_explicit_memory_candidates(question: &str) -> Vec<MemoryCandidate> {
    let question = question.trim();
    if question.is_empty() {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    if let Some(value) = suffix_after_any(question, &["记住", "记一下", "请记得"]) {
        if !value.is_empty() {
            candidates.push(candidate_with_kind(
                remembered_kind(value),
                value,
                "high",
                "explicit_user",
            ));
        }
    } else if let Some(value) = suffix_after_any(question, &["我的研究目标是", "研究目标是"])
    {
        if !value.is_empty() {
            candidates.push(candidate_with_kind("goal", value, "high", "explicit_user"));
        }
    } else if let Some(value) =
        suffix_after_any(question, &["我的研究方向是", "我长期关注", "我感兴趣的是"])
    {
        if !value.is_empty() {
            candidates.push(candidate_with_kind(
                "interest",
                value,
                "high",
                "explicit_user",
            ));
        }
    } else if let Some(value) =
        suffix_after_any(question, &["我已经理解", "我已经懂", "我熟悉", "我知道"])
    {
        if !value.is_empty() {
            candidates.push(candidate_with_kind(
                "known_concept",
                value.trim_end_matches('了'),
                "high",
                "explicit_user",
            ));
        }
    } else if let Some(value) = suffix_after_any(
        question,
        &[
            "还是不清楚",
            "还是不懂",
            "还是没懂",
            "我不清楚",
            "我不懂",
            "我没懂",
            "你这根本没讲清楚",
            "你没讲清楚",
            "没讲清楚",
            "不清楚",
            "不懂",
            "没懂",
        ],
    ) {
        candidates.push(candidate_with_kind(
            "feedback",
            if value.is_empty() { question } else { value },
            "medium",
            "explicit_user",
        ));
    }
    candidates.retain(|candidate| !candidate.value.is_empty());
    candidates
}

pub fn select_context_memories(
    items: &[MemoryItem],
    question: &str,
    limit: usize,
) -> Vec<MemoryItem> {
    let terms = question_terms(question);
    let mut ranked = items
        .iter()
        .filter(|item| item.status == "active" && !is_expired(item.expires_at.as_deref()))
        .map(|item| {
            let value = item.value.to_lowercase();
            let question_lower = question.to_lowercase();
            let term_score = terms
                .iter()
                .filter(|term| value.contains(term.as_str()))
                .count();
            let direct_score = usize::from(
                !value.is_empty()
                    && (question_lower.contains(&value)
                        || terms.iter().any(|term| term.contains(&value))),
            );
            let kind_score = match item.kind.as_str() {
                "preference" | "goal" => 3,
                "unresolved_concept" | "feedback" => 2,
                _ => 1,
            };
            let score = direct_score * 100 + term_score * 10 + kind_score;
            (score, item)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, item)| item.clone())
        .collect()
}

fn remembered_kind(value: &str) -> &'static str {
    if value.contains("研究目标") || value.contains("目标是") {
        "goal"
    } else if value.contains("感兴趣") || value.contains("研究方向") || value.contains("关注")
    {
        "interest"
    } else {
        "preference"
    }
}

fn candidate_with_kind(kind: &str, value: &str, confidence: &str, source: &str) -> MemoryCandidate {
    let value = value
        .trim()
        .trim_end_matches(|character| matches!(character, '。' | '！' | '!' | '？' | '?'))
        .chars()
        .take(MEMORY_CANDIDATE_MAX_CHARS)
        .collect();
    MemoryCandidate {
        scope_type: "global".into(),
        kind: kind.into(),
        value,
        source: source.into(),
        confidence: confidence.into(),
    }
}

fn is_expired(expires_at: Option<&str>) -> bool {
    let Some(expires_at) = expires_at else {
        return false;
    };
    let parsed = DateTime::parse_from_rfc3339(expires_at)
        .map(|value| value.with_timezone(&Utc))
        .or_else(|_| {
            NaiveDateTime::parse_from_str(expires_at, "%Y-%m-%d %H:%M:%S")
                .map(|value| DateTime::<Utc>::from_naive_utc_and_offset(value, Utc))
        });
    parsed.map_or(true, |value| value <= Utc::now())
}

fn suffix_after_any<'a>(value: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes
        .iter()
        .find_map(|prefix| value.strip_prefix(prefix).map(str::trim))
}

fn question_terms(value: &str) -> Vec<String> {
    value
        .split(|character: char| {
            character.is_whitespace() || "，。！？、：:()（）".contains(character)
        })
        .map(str::trim)
        .filter(|term| term.chars().count() >= 2)
        .map(str::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{extract_explicit_memory_candidates, select_context_memories};
    use crate::conversations::MemoryItem;

    fn item(value: &str, updated_at: &str) -> MemoryItem {
        MemoryItem {
            id: value.into(),
            scope_type: "global".into(),
            scope_id: None,
            kind: "interest".into(),
            value: value.into(),
            source: "explicit_user".into(),
            confidence: "high".into(),
            status: "active".into(),
            expires_at: None,
            created_at: updated_at.into(),
            updated_at: updated_at.into(),
        }
    }

    #[test]
    fn extracts_only_direct_user_memory_signals() {
        let remembered = extract_explicit_memory_candidates("记住我喜欢详细解释");
        assert_eq!(remembered[0].kind, "preference");
        assert_eq!(remembered[0].value, "我喜欢详细解释");
        let goal = extract_explicit_memory_candidates("我的研究目标是理解 3D VLA");
        assert_eq!(goal[0].kind, "goal");
        let feedback = extract_explicit_memory_candidates("还是不清楚稀疏卷积");
        assert_eq!(feedback[0].kind, "feedback");
        assert_eq!(feedback[0].source, "explicit_user");
        assert_eq!(feedback[0].confidence, "medium");
        assert_eq!(feedback[0].value, "稀疏卷积");
        let known = extract_explicit_memory_candidates("我已经理解稀疏张量了");
        assert_eq!(known[0].kind, "known_concept");
        assert_eq!(known[0].value, "稀疏张量");
        assert!(extract_explicit_memory_candidates("论文说作者不懂稀疏卷积").is_empty());
    }

    #[test]
    fn selects_relevant_memories_before_recent_unrelated_items() {
        let memories = vec![item("3D VLA", "2026-08-01"), item("摄影", "2026-08-14")];
        let selected = select_context_memories(&memories, "解释 3D VLA encoder", 1);
        assert_eq!(selected[0].value, "3D VLA");
    }

    #[test]
    fn excludes_dismissed_and_expired_memories() {
        let active = item("稀疏卷积", "2026-08-14 00:00:00");
        let mut dismissed = item("已隐藏", "2026-08-15 00:00:00");
        dismissed.status = "dismissed".into();
        let mut expired = item("已过期", "2026-08-15 00:00:00");
        expired.expires_at = Some("2020-01-01T00:00:00Z".into());
        let selected = select_context_memories(&[dismissed, expired, active], "稀疏卷积", 3);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].value, "稀疏卷积");
    }
}

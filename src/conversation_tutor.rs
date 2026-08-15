use crate::conversations::{ChatMessage, MemoryItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeachingIntent {
    Fact,
    Foundation,
    Mechanism,
    Comparison,
    Literature,
    FollowUp,
}

impl TeachingIntent {
    pub fn classify(
        question: &str,
        recent_history: &[ChatMessage],
        learning_state: &[MemoryItem],
    ) -> Self {
        let question = question.trim().to_lowercase();
        if contains_any(
            &question,
            &[
                "找论文",
                "搜索论文",
                "检索",
                "相关工作",
                "推荐论文",
                "literature",
            ],
        ) {
            return Self::Literature;
        }
        if contains_any(
            &question,
            &[
                "一句话回答",
                "一句话说",
                "只说结论",
                "只告诉我",
                "简短回答",
                "简要回答",
                "brief answer",
            ],
        ) {
            return Self::Fact;
        }
        if contains_any(
            &question,
            &[
                "没讲清楚",
                "没懂",
                "不懂",
                "还是不清楚",
                "换个例子",
                "重新解释",
            ],
        ) {
            return Self::FollowUp;
        }
        if contains_any(
            &question,
            &["对比", "比较", "区别", "差异", "哪个好", "versus", " vs "],
        ) {
            return Self::Comparison;
        }
        if contains_any(
            &question,
            &[
                "网络设计",
                "网络结构",
                "架构",
                "数据流",
                "怎么工作",
                "如何工作",
                "编码器是什么",
                "encoder 是什么",
                "pipeline",
            ],
        ) {
            return Self::Mechanism;
        }
        if contains_any(&question, &["什么是", "什么意思", "为什么", "怎么理解"]) {
            return Self::Foundation;
        }
        if learning_state.iter().any(|item| {
            matches!(item.kind.as_str(), "unresolved_concept" | "feedback")
                && (question.contains(&item.value.to_lowercase())
                    || question
                        .split_whitespace()
                        .any(|term| term.chars().count() >= 2 && item.value.contains(term)))
        }) || recent_history.iter().rev().take(4).any(|message| {
            message.role == "user"
                && contains_any(&message.content, &["没懂", "不清楚", "没讲清楚"])
        }) {
            return Self::FollowUp;
        }
        Self::Fact
    }
}

pub fn teaching_contract(intent: TeachingIntent) -> &'static str {
    match intent {
        TeachingIntent::Mechanism => {
            "本轮是机制或网络架构教学。先给一句话核心模型和完整数据流，再逐层解释每一阶段的输入、变化、输出及设计原因；主动补足理解后续步骤所必需的前置概念，并用最小具体例子或对比连接各层。最后再给公式、论文证据、实现边界和压缩总结。不要只报模块名称，也不要先展开与当前理解无关的项目定位。"
        }
        TeachingIntent::FollowUp => {
            "本轮是未解决追问。定位上一轮仍未建立的概念，从该处换一种表述、例子或视角重新解释；先修复前置概念，再回到整体网络中的位置。不要重复上一轮原文，也不要重新开始泛化介绍。"
        }
        TeachingIntent::Foundation => {
            "本轮是基础概念教学。先给可操作的一句话直觉，再给最小例子和常见反例，最后连接到当前论文或网络中的实际作用。通用知识不需要装饰性引用。"
        }
        TeachingIntent::Comparison => {
            "本轮是比较分析。围绕用户真正关心的轴逐项比较，先说明最关键差异和适用边界，不要堆叠各模型简介。"
        }
        TeachingIntent::Literature => {
            "本轮是外部文献检索。先明确检索问题和筛选轴，再检索、查证并组织真正相关的结果。"
        }
        TeachingIntent::Fact => {
            "本轮主要是事实确认。直接回答事实；如果上下文显示用户正在学习该机制，补一张最小数据流图和必要的设计原因，而不是只给名词。"
        }
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::{teaching_contract, MemoryItem, TeachingIntent};

    #[test]
    fn routes_architecture_and_failed_explanations_to_teaching() {
        assert_eq!(
            TeachingIntent::classify("AnyGrasp 的编码器是什么", &[], &[]),
            TeachingIntent::Mechanism
        );
        assert_eq!(
            TeachingIntent::classify("你这根本没讲清楚网络设计", &[], &[]),
            TeachingIntent::FollowUp
        );
        assert_eq!(
            TeachingIntent::classify("什么是稀疏三维卷积", &[], &[]),
            TeachingIntent::Foundation
        );
        assert!(teaching_contract(TeachingIntent::Mechanism).contains("完整数据流"));
    }

    #[test]
    fn explicit_brevity_overrides_the_expanded_teaching_contract() {
        assert_eq!(
            TeachingIntent::classify("只用一句话回答：AnyGrasp 的编码器是什么", &[], &[]),
            TeachingIntent::Fact
        );
    }

    #[test]
    fn project_feedback_can_route_a_related_follow_up_without_requiring_exact_history() {
        let feedback = MemoryItem {
            id: "feedback".into(),
            scope_type: "project".into(),
            scope_id: Some("project".into()),
            kind: "feedback".into(),
            value: "稀疏卷积".into(),
            source: "explicit_user".into(),
            confidence: "medium".into(),
            status: "active".into(),
            expires_at: None,
            created_at: "2026-08-15 00:00:00".into(),
            updated_at: "2026-08-15 00:00:00".into(),
        };
        assert_eq!(
            TeachingIntent::classify("继续解释稀疏卷积", &[], &[feedback]),
            TeachingIntent::FollowUp
        );
    }

    #[test]
    fn anygrasp_fixture_preserves_the_three_turn_teaching_sequence() {
        let fixture = include_str!("../tests/fixtures/teaching/anygrasp.md");
        assert!(fixture.contains("AnyGrasp 的编码器是什么"));
        assert!(fixture.contains("你这根本没讲清楚网络设计"));
        assert!(fixture.contains("什么是稀疏三维卷积"));
        assert_eq!(
            TeachingIntent::classify("AnyGrasp 的编码器是什么", &[], &[]),
            TeachingIntent::Mechanism
        );
        assert_eq!(
            TeachingIntent::classify("你这根本没讲清楚网络设计", &[], &[]),
            TeachingIntent::FollowUp
        );
        assert_eq!(
            TeachingIntent::classify("什么是稀疏三维卷积", &[], &[]),
            TeachingIntent::Foundation
        );
    }
}

//! Versioned bundled defaults, with workspace-local text overrides read each run.
use crate::{codex::CodexSkillSelection, workspace::Workspace};
use anyhow::{Context, Result};

pub(crate) const NAME: &str = "project-morning-briefing";
pub(crate) const ENTRY: &str =
    include_str!("../workspace-template/.codex/skills/project-morning-briefing/SKILL.md");
pub(crate) const PLANNING: &str = include_str!(
    "../workspace-template/.codex/skills/project-morning-briefing/references/planning.md"
);
pub(crate) const EDITORIAL: &str = include_str!(
    "../workspace-template/.codex/skills/project-morning-briefing/references/editorial.md"
);

pub(crate) async fn instructions(workspace: &Workspace, planning: bool) -> Result<String> {
    let base = workspace.root().join(".codex/skills").join(NAME);
    let entry = tokio::fs::read_to_string(base.join("SKILL.md"))
        .await
        .context("读取晨报 skill")?;
    let reference = tokio::fs::read_to_string(base.join(if planning {
        "references/planning.md"
    } else {
        "references/editorial.md"
    }))
    .await
    .context("读取晨报阶段说明")?;
    Ok(format!("{entry}\n\n{reference}"))
}

pub(crate) fn selection(workspace: &Workspace) -> CodexSkillSelection {
    CodexSkillSelection {
        name: NAME.into(),
        path: workspace
            .root()
            .join(".codex/skills")
            .join(NAME)
            .join("SKILL.md"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn stage_specific_skill_text_is_reloaded_without_reinitializing_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = Workspace::initialize(directory.path()).await.unwrap();
        let editor = workspace
            .root()
            .join(".codex/skills/project-morning-briefing/references/editorial.md");
        tokio::fs::write(&editor, "EDITORIAL_MARKER_ONE")
            .await
            .unwrap();
        assert!(instructions(&workspace, false)
            .await
            .unwrap()
            .contains("EDITORIAL_MARKER_ONE"));
        assert!(!instructions(&workspace, true)
            .await
            .unwrap()
            .contains("EDITORIAL_MARKER_ONE"));
        tokio::fs::write(&editor, "EDITORIAL_MARKER_TWO")
            .await
            .unwrap();
        let changed = instructions(&workspace, false).await.unwrap();
        assert!(changed.contains("EDITORIAL_MARKER_TWO"));
        assert!(!changed.contains("EDITORIAL_MARKER_ONE"));
        assert!(selection(&workspace).path.is_file());
    }
}

import type { Dashboard } from "./types"
import type { CodexSelection } from "./conversation-scope"

export function projectIdForSelection(selection: CodexSelection): string | undefined {
  if (selection.kind === "project" && selection.id) return selection.id
  return selection.projectId
}

export function withProjectContext(
  selection: CodexSelection,
  currentProjectId: string | undefined,
  dashboard: Dashboard,
): CodexSelection {
  if (selection.kind === "project" && selection.id) {
    return { ...selection, projectId: selection.id }
  }
  const projectId =
    selection.projectId ??
    currentProjectId ??
    dashboard.projects[0]?.id
  return projectId ? { ...selection, projectId } : selection
}


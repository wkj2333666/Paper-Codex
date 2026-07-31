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
  const known=(projectId:string|undefined)=>projectId&&dashboard.projects.some(project=>project.id===projectId)?projectId:undefined
  const projectId =
    known(selection.projectId) ??
    known(currentProjectId) ??
    dashboard.projects[0]?.id
  return projectId ? { ...selection, projectId } : selection
}

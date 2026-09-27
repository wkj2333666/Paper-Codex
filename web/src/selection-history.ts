import type { CodexSelection } from "./conversation-scope"

export const SELECTION_HISTORY_KEY = "paperCodexSelection"

export type SelectionHistoryState = { [SELECTION_HISTORY_KEY]: CodexSelection }

const SELECTION_KINDS = new Set(["workbench", "briefing", "inbox", "paper", "project", "search", "graph", "trash"])

export function selectionHistoryState(selection: CodexSelection): SelectionHistoryState {
  return { [SELECTION_HISTORY_KEY]: selection }
}

export function selectionFromHistoryState(value: unknown): CodexSelection | null {
  if (!value || typeof value !== "object") return null
  const selection = (value as Record<string, unknown>)[SELECTION_HISTORY_KEY]
  if (!selection || typeof selection !== "object") return null
  const kind = (selection as Record<string, unknown>).kind
  if (typeof kind !== "string" || !SELECTION_KINDS.has(kind)) return null

  const restored = selection as Partial<CodexSelection>
  if (restored.id !== undefined && typeof restored.id !== "string") return null
  if (restored.projectId !== undefined && typeof restored.projectId !== "string") return null
  if ((kind === "paper" || kind === "project") && !restored.id) return null

  return {
    kind: kind as CodexSelection["kind"],
    ...(restored.id ? { id: restored.id } : {}),
    ...(restored.projectId ? { projectId: restored.projectId } : {}),
  }
}

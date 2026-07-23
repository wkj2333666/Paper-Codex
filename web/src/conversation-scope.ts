import type { ConversationScope } from "./types"

export interface CodexSelection { kind: "workbench"|"inbox"|"paper"|"project"|"search"|"graph"|"trash"; id?: string }

function scopeKey(scope: ConversationScope): string | null {
  if (scope.scope_type === "global") return "global"
  if (!scope.scope_id) return null
  return `${scope.scope_type}:${scope.scope_id}`
}

function selectionKey(selection: CodexSelection): string {
  if (selection.kind === "paper" && selection.id) return `paper:${selection.id}`
  if (selection.kind === "project" && selection.id) return `project:${selection.id}`
  return "global"
}

export function selectionForScopes(scopes: ConversationScope[]): CodexSelection | null {
  for (const scope of scopes) {
    if (scope.scope_type === "paper" && scope.scope_id) return { kind: "paper", id: scope.scope_id }
    if (scope.scope_type === "project" && scope.scope_id) return { kind: "project", id: scope.scope_id }
    if (scope.scope_type === "global") return { kind: "workbench" }
  }
  return null
}

export function scopesMatchSelection(scopes: ConversationScope[], selection: CodexSelection): boolean {
  const current = selectionKey(selection)
  return scopes.some(scope => scopeKey(scope) === current)
}

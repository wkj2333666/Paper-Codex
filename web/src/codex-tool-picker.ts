import type { CodexIntegrations, CodexSkill, CodexToolPreference } from "./types"

export interface CompletionRange {
  start: number
  end: number
  query: string
}

export type CodexToolCatalogItem =
  | {
      kind: "skill"
      key: string
      label: string
      description: string
      skill: CodexSkill
    }
  | {
      kind: "mcp-tool"
      key: string
      label: string
      description: string
      serverLabel: string
      preference: CodexToolPreference
    }

export type ToolPickerKeyIntent = "next" | "previous" | "select" | "close" | "ignore"

export function buildCodexToolCatalog(integrations: CodexIntegrations | null): CodexToolCatalogItem[] {
  if (!integrations) return []
  const seen = new Set<string>()
  const items: CodexToolCatalogItem[] = []
  for (const skill of integrations.skills) {
    if (!skill.enabled) continue
    const key = `skill:${skill.path}`
    if (seen.has(key)) continue
    seen.add(key)
    items.push({
      kind: "skill",
      key,
      label: skill.display_name || skill.name,
      description: skill.description || skill.name,
      skill,
    })
  }
  for (const server of integrations.mcp_servers) {
    for (const tool of server.tools) {
      const key = `mcp:${server.name}:${tool.name}`
      if (seen.has(key)) continue
      seen.add(key)
      items.push({
        kind: "mcp-tool",
        key,
        label: tool.title || tool.name,
        description: tool.description || tool.name,
        serverLabel: server.title || server.name,
        preference: { server: server.name, tool: tool.name },
      })
    }
  }
  return items
}

export function filterCodexToolCatalog(
  items: CodexToolCatalogItem[],
  query: string,
): CodexToolCatalogItem[] {
  const normalized = query.trim().toLocaleLowerCase()
  if (!normalized) return items
  return items.filter((item) => {
    const haystack =
      item.kind === "skill"
        ? [item.skill.name, item.label, item.description, item.skill.scope]
        : [item.preference.server, item.preference.tool, item.serverLabel, item.label, item.description]
    return haystack.join("\n").toLocaleLowerCase().includes(normalized)
  })
}

export function completionAtCursor(text: string, cursor: number): CompletionRange | null {
  if (!Number.isInteger(cursor) || cursor < 0 || cursor > text.length) return null
  const before = text.slice(0, cursor)
  const match = before.match(/(?:^|\s)(\$[^\s$]*)$/u)
  if (!match) return null
  const token = match[1]
  const start = cursor - token.length
  let end = cursor
  while (end < text.length && !/\s/u.test(text[end])) end += 1
  return { start, end, query: token.slice(1) }
}

export function applyCompletion(
  text: string,
  range: CompletionRange,
  mention: string,
): { text: string; cursor: number } {
  const next = `${text.slice(0, range.start)}${mention}${text.slice(range.end)}`
  return { text: next, cursor: range.start + mention.length }
}

export function toolPickerKeyIntent(key: string, shiftKey: boolean): ToolPickerKeyIntent {
  if (key === "ArrowDown") return "next"
  if (key === "ArrowUp") return "previous"
  if (key === "Enter" && !shiftKey) return "select"
  if (key === "Escape") return "close"
  return "ignore"
}

export function codexToolOptionId(item: CodexToolCatalogItem): string {
  const safe = (value: string) => value.replace(/[^A-Za-z0-9_-]+/g, "-")
  return item.kind === "skill"
    ? `codex-tool-option-skill-${safe(item.skill.name)}`
    : `codex-tool-option-mcp-${safe(item.preference.server)}-${safe(item.preference.tool)}`
}

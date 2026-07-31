import { describe, expect, it } from "vitest"
import {
  applyCompletion,
  buildCodexToolCatalog,
  completionAtCursor,
  filterCodexToolCatalog,
  toolPickerKeyIntent,
} from "./codex-tool-picker"
import type { CodexIntegrations } from "./types"

const integrations: CodexIntegrations = {
  skills: [
    {
      name: "paper-research",
      display_name: "Paper Research",
      description: "基于证据阅读与比较论文",
      path: "/workspace/.codex/skills/paper-research/SKILL.md",
      scope: "repo",
      enabled: true,
      dependencies: [],
    },
    {
      name: "disabled-skill",
      display_name: "Disabled",
      description: "",
      path: "/workspace/.codex/skills/disabled/SKILL.md",
      scope: "user",
      enabled: false,
      dependencies: [],
    },
  ],
  mcp_servers: [
    {
      name: "openalex",
      title: "OpenAlex",
      description: "学术检索",
      auth_status: "oAuth",
      tools: [
        { name: "works/search", title: "Search works", description: "检索相关论文" },
        { name: "works/search", title: "Search works duplicate", description: "重复项" },
      ],
    },
    {
      name: "empty",
      title: "Empty",
      description: null,
      auth_status: "unsupported",
      tools: [],
    },
  ],
  supports_skills: true,
  supports_mcp_status: true,
  skills_error: null,
  mcp_error: null,
}

describe("Codex tool catalog", () => {
  it("flattens enabled Skills and concrete MCP tools with stable deduplicated keys", () => {
    const items = buildCodexToolCatalog(integrations)
    expect(items.map((item) => item.key)).toEqual([
      "skill:/workspace/.codex/skills/paper-research/SKILL.md",
      "mcp:openalex:works/search",
    ])
  })

  it("filters names, labels, descriptions, and server labels without mutating the catalog", () => {
    const items = buildCodexToolCatalog(integrations)
    expect(filterCodexToolCatalog(items, "证据").map((item) => item.key)).toEqual([items[0].key])
    expect(filterCodexToolCatalog(items, "openalex").map((item) => item.key)).toEqual([items[1].key])
    expect(items).toHaveLength(2)
  })
})

describe("$ completion", () => {
  it("recognizes only the dollar token immediately before the cursor", () => {
    expect(completionAtCursor("比较 $pap 的证据", 7)).toEqual({ start: 3, end: 7, query: "pap" })
    expect(completionAtCursor("价格是 10$ 不触发", 7)).toBeNull()
    expect(completionAtCursor("比较 $pap 后续", 10)).toBeNull()
  })

  it("replaces the completion range and returns the next cursor position", () => {
    expect(
      applyCompletion("比较 $pap 的证据", { start: 3, end: 7, query: "pap" }, "$paper-research"),
    ).toEqual({
      text: "比较 $paper-research 的证据",
      cursor: 18,
    })
  })

  it("maps keyboard events without stealing Shift+Enter", () => {
    expect(toolPickerKeyIntent("ArrowDown", false)).toBe("next")
    expect(toolPickerKeyIntent("ArrowUp", false)).toBe("previous")
    expect(toolPickerKeyIntent("Enter", false)).toBe("select")
    expect(toolPickerKeyIntent("Enter", true)).toBe("ignore")
    expect(toolPickerKeyIntent("Escape", false)).toBe("close")
  })
})

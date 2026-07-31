import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { CodexToolPicker } from "./CodexToolPicker"
import { buildCodexToolCatalog } from "./codex-tool-picker"
import type { CodexIntegrations } from "./types"

const integrations: CodexIntegrations = {
  skills: [{
    name: "paper-research",
    display_name: "Paper Research",
    description: "论文检索与综合",
    path: "/workspace/.codex/skills/paper-research/SKILL.md",
    scope: "repo",
    enabled: true,
    dependencies: [],
  }],
  mcp_servers: [{
    name: "ssh-bridge",
    title: "SSH Bridge",
    description: "远程项目访问",
    auth_status: "unsupported",
    tools: [{ name: "remote_read", title: "Read remote files", description: "读取远端文件" }],
  }],
  supports_skills: true,
  supports_mcp_status: true,
  skills_error: null,
  mcp_error: null,
}

describe("CodexToolPicker", () => {
  it("renders grouped searchable options with an active descendant", () => {
    const html = renderToStaticMarkup(
      <CodexToolPicker
        open
        items={buildCodexToolCatalog(integrations)}
        query=""
        activeIndex={1}
        loading={false}
        onQuery={() => {}}
        onActiveIndex={() => {}}
        onSelect={() => {}}
        onClose={() => {}}
      />,
    )

    expect(html).toContain("搜索 Skills 和工具")
    expect(html).toContain("Skills")
    expect(html).toContain("MCP 工具")
    expect(html).toContain("Paper Research")
    expect(html).toContain("SSH Bridge")
    expect(html).toContain("Read remote files")
    expect(html).toContain('aria-activedescendant="codex-tool-option-mcp-ssh-bridge-remote_read"')
  })

  it("keeps a useful empty state while integrations are unavailable", () => {
    const html = renderToStaticMarkup(
      <CodexToolPicker
        open
        items={[]}
        query="missing"
        activeIndex={0}
        loading={false}
        onQuery={() => {}}
        onActiveIndex={() => {}}
        onSelect={() => {}}
        onClose={() => {}}
      />,
    )
    expect(html).toContain("没有匹配的 Skill 或工具")
  })
})

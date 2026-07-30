import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { CodexIntegrationsDrawer, filterCodexSkills } from "./CodexIntegrationsDrawer"
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
      name: "private-writer",
      display_name: "Private Writer",
      description: "需要额外工具",
      path: "/workspace/.codex/skills/private-writer/SKILL.md",
      scope: "user",
      enabled: false,
      dependencies: ["mcp:private"],
    },
  ],
  mcp_servers: [
    {
      name: "openalex",
      title: "OpenAlex",
      description: "检索学术元数据",
      auth_status: "oAuth",
      tools: [{ name: "works/search", title: "Search works", description: "检索论文" }],
    },
    {
      name: "private",
      title: null,
      description: null,
      auth_status: "notLoggedIn",
      tools: [],
    },
  ],
  supports_skills: true,
  supports_mcp_status: true,
  skills_error: null,
  mcp_error: null,
}

describe("CodexIntegrationsDrawer", () => {
  it("shows selectable Skills and read-only MCP status without direct call buttons", () => {
    const html = renderToStaticMarkup(
      <CodexIntegrationsDrawer
        open
        integrations={integrations}
        loading={false}
        selectedSkill={integrations.skills[0]}
        onClose={() => {}}
        onRefresh={() => {}}
        onSelectSkill={() => {}}
      />,
    )
    expect(html).toContain("Codex 能力")
    expect(html).toContain("Paper Research")
    expect(html).toContain("已选择")
    expect(html).toContain("Private Writer")
    expect(html).toContain("disabled")
    expect(html).toContain("OpenAlex")
    expect(html).toContain("OAuth 已连接")
    expect(html).toContain("需要登录")
    expect(html).toContain("works/search")
    expect(html).not.toContain("直接调用")
  })

  it("keeps one capability section usable when the other reports an error", () => {
    const html = renderToStaticMarkup(
      <CodexIntegrationsDrawer
        open
        integrations={{ ...integrations, skills: [], skills_error: "读取 Skills 失败" }}
        loading={false}
        selectedSkill={null}
        onClose={() => {}}
        onRefresh={() => {}}
        onSelectSkill={() => {}}
      />,
    )
    expect(html).toContain("读取 Skills 失败")
    expect(html).toContain("OpenAlex")
  })

  it("filters by name and description without changing the source inventory", () => {
    expect(filterCodexSkills(integrations.skills, "证据").map((skill) => skill.name)).toEqual([
      "paper-research",
    ])
    expect(filterCodexSkills(integrations.skills, "private").map((skill) => skill.name)).toEqual([
      "private-writer",
    ])
    expect(integrations.skills).toHaveLength(2)
  })
})

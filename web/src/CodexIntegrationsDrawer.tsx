import { useMemo, useState } from "react"
import { Blocks, LoaderCircle, RefreshCw, Search, Sparkles, Wrench, X } from "lucide-react"
import type { CodexIntegrations, CodexMcpServer, CodexSkill } from "./types"

export function filterCodexSkills(skills: CodexSkill[], query: string): CodexSkill[] {
  const normalized = query.trim().toLocaleLowerCase()
  if (!normalized) return skills
  return skills.filter((skill) =>
    [skill.name, skill.display_name, skill.description]
      .join("\n")
      .toLocaleLowerCase()
      .includes(normalized),
  )
}

const skillScopeLabels: Record<string, string> = {
  repo: "当前工作区",
  user: "个人",
  system: "系统",
  admin: "管理员",
}
const skillScopeLabel = (scope: string) => skillScopeLabels[scope] ?? scope

const mcpAuthLabels: Record<string, string> = {
  oAuth: "OAuth 已连接",
  bearerToken: "令牌已配置",
  notLoggedIn: "需要登录",
  unsupported: "无需认证",
}
const mcpAuthLabel = (server: CodexMcpServer) => mcpAuthLabels[server.auth_status] ?? server.auth_status

interface CodexIntegrationsDrawerProps {
  open: boolean
  integrations: CodexIntegrations | null
  loading: boolean
  selectedSkill: CodexSkill | null
  onClose: () => void
  onRefresh: () => void
  onSelectSkill: (skill: CodexSkill | null) => void
}

export function CodexIntegrationsDrawer({
  open,
  integrations,
  loading,
  selectedSkill,
  onClose,
  onRefresh,
  onSelectSkill,
}: CodexIntegrationsDrawerProps) {
  const [query, setQuery] = useState("")
  const skills = useMemo(
    () => filterCodexSkills(integrations?.skills ?? [], query),
    [integrations?.skills, query],
  )
  if (!open) return null
  return (
    <section className="codex-integrations-drawer" aria-label="Codex 能力">
      <header>
        <div>
          <Blocks />
          <div>
            <strong>Codex 能力</strong>
            <span>当前研究工作区</span>
          </div>
        </div>
        <div className="codex-integrations-actions">
          <button type="button" aria-label="刷新 Codex 能力" onClick={onRefresh} disabled={loading}>
            <RefreshCw className={loading ? "spin" : ""} />
          </button>
          <button type="button" aria-label="关闭 Codex 能力" onClick={onClose}>
            <X />
          </button>
        </div>
      </header>

      <div className="codex-integrations-scroll">
        <section className="codex-integration-section" aria-labelledby="codex-skills-heading">
          <div className="codex-integration-heading">
            <Sparkles />
            <div>
              <strong id="codex-skills-heading">Skills</strong>
              <span>为下一轮对话选择工作流</span>
            </div>
          </div>
          <label className="codex-skill-search">
            <Search />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="搜索 Skill"
              aria-label="搜索 Skill"
            />
          </label>
          {integrations?.skills_error && <p className="codex-integration-error">{integrations.skills_error}</p>}
          {loading && !integrations ? (
            <div className="codex-integrations-loading">
              <LoaderCircle className="spin" />
              <span>正在读取 Codex 能力…</span>
            </div>
          ) : skills.length ? (
            <div className="codex-skill-list">
              {skills.map((skill) => {
                const selected = selectedSkill?.name === skill.name && selectedSkill.path === skill.path
                return (
                  <button
                    type="button"
                    key={`${skill.scope}:${skill.path}`}
                    className={selected ? "selected" : ""}
                    disabled={!skill.enabled}
                    aria-pressed={selected}
                    onClick={() => onSelectSkill(selected ? null : skill)}
                  >
                    <span className="codex-skill-icon">
                      <Sparkles />
                    </span>
                    <span className="codex-skill-copy">
                      <span>
                        <strong>{skill.display_name}</strong>
                        <small>{skillScopeLabel(skill.scope)}</small>
                      </span>
                      <span>{skill.description || skill.name}</span>
                      {skill.dependencies.length > 0 && (
                        <small>依赖：{skill.dependencies.join("、")}</small>
                      )}
                    </span>
                    {selected && <b>已选择</b>}
                  </button>
                )
              })}
            </div>
          ) : (
            <p className="codex-integration-empty">
              {query ? "没有匹配的 Skill" : "当前作用域没有可用 Skill"}
            </p>
          )}
        </section>

        <section className="codex-integration-section" aria-labelledby="codex-mcp-heading">
          <div className="codex-integration-heading">
            <Wrench />
            <div>
              <strong id="codex-mcp-heading">MCP</strong>
              <span>由 Codex 配置提供，只读显示</span>
            </div>
          </div>
          {integrations?.mcp_error && <p className="codex-integration-error">{integrations.mcp_error}</p>}
          {(integrations?.mcp_servers.length ?? 0) > 0 ? (
            <div className="codex-mcp-list">
              {integrations!.mcp_servers.map((server) => (
                <details key={server.name}>
                  <summary>
                    <span>
                      <strong>{server.title || server.name}</strong>
                      <small>{server.tools.length} 个工具</small>
                    </span>
                    <b className={`auth-${server.auth_status}`}>{mcpAuthLabel(server)}</b>
                  </summary>
                  {server.description && <p>{server.description}</p>}
                  {server.tools.length ? (
                    <ul>
                      {server.tools.map((tool) => (
                        <li key={tool.name}>
                          <strong>{tool.title || tool.name}</strong>
                          {tool.title && <code>{tool.name}</code>}
                          {tool.description && <span>{tool.description}</span>}
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p>当前没有可用工具</p>
                  )}
                </details>
              ))}
            </div>
          ) : (
            <p className="codex-integration-empty">当前没有已配置的 MCP 服务器</p>
          )}
        </section>
      </div>
    </section>
  )
}

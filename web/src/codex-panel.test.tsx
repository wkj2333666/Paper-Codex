import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { CodexPanel, ConversationProgress } from "./CodexPanel"

describe("CodexPanel", () => {
  it("defaults to a conversation composer with history and activity controls", () => {
    const html = renderToStaticMarkup(
      <CodexPanel
        selection={{ kind: "paper", id: "paper:one" }}
        scopeLabel="Attention Is All You Need"
        activities={[]}
        drawerOpen={false}
        onCollapse={() => {}}
        onCitation={() => {}}
        onCitations={() => {}}
        onSelect={() => {}}
      />,
    )
    expect(html).toContain("新建对话")
    expect(html).toContain("对话历史")
    expect(html).toContain("活动记录")
    expect(html).toContain("询问这篇论文")
    expect(html).toContain('data-testid="codex-scope"')
    expect(html).toContain("当前作用域")
    expect(html).toContain("Attention Is All You Need")
    expect(html).toContain('aria-label="发送消息"')
  })

  it("shows application progress without exposing model reasoning", () => {
    const reading = renderToStaticMarkup(<ConversationProgress phase="reading" />)
    const reasoning = renderToStaticMarkup(<ConversationProgress phase="reasoning" />)
    expect(reading).toContain("Codex 正在读取论文")
    expect(reasoning).toContain("Codex 正在分析证据并组织回答")
    expect(reasoning).not.toContain("chain-of-thought")
  })
})

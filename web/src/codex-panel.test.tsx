import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { CodexPanel, ConversationProgress } from "./CodexPanel"
import { CodexMessage } from "./CodexMessage"
import type { ChatMessage } from "./types"

const capabilities = {
  default:{model:"gpt-test", reasoning_effort:"medium", service_tier:null},
  models:[{id:"gpt-test",display_name:"GPT Test",default_reasoning_effort:"medium",supported_reasoning_efforts:["low","medium","high"],supports_fast:true}],
  supports_dynamic_tools:true,
}

const message = (overrides: Partial<ChatMessage>): ChatMessage => ({
  id: "message-1",
  conversation_id: "conversation-1",
  role: "assistant",
  content: "最终回答",
  turn_id: "turn-1",
  status: "completed",
  error: null,
  research_mode: "auto",
  citations: [],
  candidate_citations: [],
  created_at: "2026-07-30T00:00:00Z",
  updated_at: "2026-07-30T00:00:00Z",
  ...overrides,
})

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
        codexCapabilities={capabilities}
      />,
    )
    expect(html).toContain("新建对话")
    expect(html).toContain("对话历史")
    expect(html).toContain("活动记录")
    expect(html).toContain("Codex 能力")
    expect(html).toContain("询问这篇论文")
    expect(html).toContain('data-testid="codex-scope"')
    expect(html).toContain("当前作用域")
    expect(html).toContain("Attention Is All You Need")
    expect(html).toContain('aria-label="发送消息"')
    expect(html).toContain("模型")
    expect(html).toContain("推理强度")
    expect(html).toContain("速度")
  })

  it("uses a task-oriented Codex Desktop layout", () => {
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
        codexCapabilities={capabilities}
      />,
    )
    expect(html).toContain("codex-task-header")
    expect(html).toContain("codex-scope-pill")
    expect(html).toContain("codex-empty-prompts")
    expect(html).toContain("codex-composer-context")
    expect(html).toContain("可以这样开始")
    expect(html).not.toContain("codex-subnav")
  })

  it("shows application progress without exposing model reasoning", () => {
    const reading = renderToStaticMarkup(<ConversationProgress phase="reading" />)
    const reasoning = renderToStaticMarkup(<ConversationProgress phase="reasoning" />)
    expect(reading).toContain("工作过程")
    expect(reading).toContain("Codex 正在读取论文")
    expect(reasoning).toContain("Codex 正在分析证据并组织回答")
    expect(reasoning).not.toContain("chain-of-thought")
  })

  it("renders user prompts, completed answers, and live work as distinct surfaces", () => {
    const user = renderToStaticMarkup(
      <CodexMessage message={message({ role: "user", content: "为什么选择这个游戏？", skill_name:"paper-research" })} onCitation={() => {}} />,
    )
    const answer = renderToStaticMarkup(
      <CodexMessage message={message({ content: "作者选择该环境是为了控制变量。" })} onCitation={() => {}} />,
    )
    const live = renderToStaticMarkup(
      <CodexMessage
        message={message({
          content: "",
          live_content: "正在核对实验设置",
          status: "streaming",
          progress_phase: "reading",
        })}
        onCitation={() => {}}
      />,
    )
    expect(user).toContain("codex-user-message")
    expect(user).toContain("你")
    expect(user).toContain("paper-research")
    expect(answer).toContain("codex-answer")
    expect(answer).toContain("Codex")
    expect(live).toContain("codex-worklog")
    expect(live).toContain("工作过程")
    expect(live).toContain("正在核对实验设置")
  })

  it("offers controlled literature search only in project scope", () => {
    const project=renderToStaticMarkup(
      <CodexPanel selection={{kind:"project",id:"project-a"}} scopeLabel="规则复杂度" activities={[]} drawerOpen={false} onCollapse={()=>{}} onCitation={()=>{}} onCandidate={()=>{}} onCitations={()=>{}} onSelect={()=>{}} codexCapabilities={capabilities}/>,
    )
    const paper=renderToStaticMarkup(
      <CodexPanel selection={{kind:"paper",id:"paper-a"}} scopeLabel="论文" activities={[]} drawerOpen={false} onCollapse={()=>{}} onCitation={()=>{}} onCandidate={()=>{}} onCitations={()=>{}} onSelect={()=>{}} codexCapabilities={capabilities}/>,
    )
    expect(project).toContain('aria-label="检索论文"')
    expect(paper).not.toContain('aria-label="检索论文"')
  })

  it("explains when controlled search is unavailable", () => {
    const html=renderToStaticMarkup(
      <CodexPanel selection={{kind:"project",id:"project-a"}} scopeLabel="规则复杂度" activities={[]} drawerOpen={false} onCollapse={()=>{}} onCitation={()=>{}} onCandidate={()=>{}} onCitations={()=>{}} onSelect={()=>{}} codexCapabilities={{...capabilities,supports_dynamic_tools:false}}/>,
    )
    expect(html).toContain("当前 Codex 版本不支持受控论文检索")
  })
})

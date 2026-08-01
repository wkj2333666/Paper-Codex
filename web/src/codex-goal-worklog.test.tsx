import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { CodexGoalBar } from "./CodexGoalBar"
import { CodexWorklog } from "./CodexWorklog"

describe("native Codex goal and work surfaces", () => {
  it("renders compact native goal progress and controls", () => {
    const html = renderToStaticMarkup(<CodexGoalBar goal={{
      thread_id: "thread-1",
      objective: "完成共享前缀相关工作综述",
      status: "active",
      token_budget: 40000,
      tokens_used: 1200,
      time_used_seconds: 35,
    }} onPause={() => {}} onResume={() => {}} onEdit={() => {}} onClear={() => {}} />)
    expect(html).toContain("完成共享前缀相关工作综述")
    expect(html).toContain("1,200 / 40,000")
    expect(html).toContain('aria-label="暂停目标"')
    expect(html).toContain('aria-label="编辑目标"')
    expect(html).toContain('aria-label="清除目标"')
  })

  it("renders only the latest readable thought while active and hides tool details", () => {
    const html = renderToStaticMarkup(<CodexWorklog active worklog={{
      summaries: [
        { item_id: "reasoning-1", summary_index: 0, text: "旧的思考" },
        { item_id: "reasoning-1", summary_index: 1, text: "正在核对论文证据" },
      ],
      plan: { explanation: "先定位再回答", steps: [
        { step: "定位证据", status: "completed" },
        { step: "组织回答", status: "inProgress" },
      ] },
      items: { "tool-1": { item_id: "tool-1", item_type: "webSearch", label: "检索论文", status: "completed" } },
    }} />)
    expect(html).toContain("正在核对论文证据")
    expect(html).not.toContain("旧的思考")
    expect(html).not.toContain("组织回答")
    expect(html).not.toContain("检索论文")
    expect(html).not.toContain("raw hidden reasoning")
  })

  it("does not retain ephemeral thinking after the answer completes", () => {
    const html = renderToStaticMarkup(<CodexWorklog active={false} worklog={{
      summaries: [{ item_id: "reasoning-1", summary_index: 0, text: "临时思考" }],
      items: {},
    }} />)
    expect(html).toBe("")
  })
})

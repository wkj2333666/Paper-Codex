import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { IntakeTaskCard } from "./IntakeTaskCard"
import type { Task } from "./types"

const task = (state: string): Task => ({
  id: `task-${state}`,
  kind: "ingest",
  state,
  input_json: JSON.stringify({ source: "arxiv:1706.03762" }),
  paper_id: null,
  project_id: null,
  thread_id: null,
  error: state === "failed" ? "下载失败" : null,
  created_at: "2026-07-19T00:00:00Z",
  updated_at: "2026-07-19T00:00:00Z",
})

describe("IntakeTaskCard", () => {
  it("offers cancellation only for active work", () => {
    const html = renderToStaticMarkup(<IntakeTaskCard task={task("analyzing")} onCancel={() => {}} onDismiss={() => {}} />)
    expect(html).toContain('aria-label="取消任务"')
    expect(html).toContain("spin")
    expect(html).not.toContain('aria-label="关闭记录"')
  })

  it("offers dismissal without a spinner for terminal failures", () => {
    const html = renderToStaticMarkup(<IntakeTaskCard task={task("failed")} onCancel={() => {}} onDismiss={() => {}} />)
    expect(html).toContain('aria-label="关闭记录"')
    expect(html).toContain("下载失败")
    expect(html).not.toContain("spin")
    expect(html).not.toContain('aria-label="取消任务"')
  })
})

import { describe, expect, it } from "vitest"
import type { StreamEvent, Task } from "./types"
import { groupIntakeTasks, intakeStateLabel, mergeIntakeTaskEvent, taskSource } from "./intake-status"

const task = (overrides: Partial<Task> = {}): Task => ({
  id: "task-1",
  kind: "ingest",
  state: "queued",
  input_json: JSON.stringify({ source: "arxiv:1706.03762", project_id: null, upload_path: null }),
  paper_id: null,
  project_id: null,
  thread_id: null,
  error: null,
  created_at: "2026-07-19T00:00:00Z",
  updated_at: "2026-07-19T00:00:00Z",
  ...overrides,
})

describe("intake status", () => {
  it("extracts the submitted source and maps processing states to Chinese", () => {
    expect(taskSource(task())).toBe("arxiv:1706.03762")
    expect(intakeStateLabel("extracting")).toBe("正在提取正文")
    expect(taskSource(task({ input_json: "{" }))).toBe("未命名论文")
  })

  it("merges a stage event into only the matching intake task", () => {
    const other = task({ id: "task-2", input_json: JSON.stringify({ source: "other" }) })
    const event: StreamEvent = { id: 1, type: "stage", task_id: "task-1", payload: { state: "analyzing" }, created_at: "now" }
    const next = mergeIntakeTaskEvent([task(), other], event)
    expect(next[0].state).toBe("analyzing")
    expect(next[1]).toEqual(other)
  })

  it("stores failure and cancellation details from terminal events", () => {
    const failed = mergeIntakeTaskEvent([task()], { id: 2, type: "failed", task_id: "task-1", payload: { message: "下载失败" }, created_at: "now" })
    expect(failed[0]).toMatchObject({ state: "failed", error: "下载失败" })
    const cancelled = mergeIntakeTaskEvent(failed, { id: 3, type: "cancelled", task_id: "task-1", payload: {}, created_at: "now" })
    expect(cancelled[0].state).toBe("cancelled")
  })

  it("keeps model fallback and analysis warnings visible on the matching task", () => {
    const switched = mergeIntakeTaskEvent([task()], {
      id: 4,
      type: "model-switch",
      task_id: "task-1",
      payload: { from: "gpt-5.6-sol", to: "gpt-5.6-terra", reason: "capacity" },
      created_at: "now",
    })
    expect(switched[0]).toMatchObject({
      analysis_model: "gpt-5.6-terra",
      status_note: "gpt-5.6-sol 容量不足，已切换至 gpt-5.6-terra",
    })

    const warned = mergeIntakeTaskEvent(switched, {
      id: 5,
      type: "analysis-warning",
      task_id: "task-1",
      payload: {
        source_key: "paper",
        relation_type: "reports",
        target_key: "method:missing",
        reason: "unresolved",
      },
      created_at: "now",
    })
    expect(warned[0].analysis_warnings).toEqual([
      "已忽略无法定位的关系：paper --reports--> method:missing",
    ])
  })

  it("records the actual completing model from the result event", () => {
    const completed = mergeIntakeTaskEvent([task()], {
      id: 6,
      type: "result",
      task_id: "task-1",
      payload: {
        title: "Paper",
        analysis_model: "gpt-5.6-terra",
        reasoning_effort: "medium",
      },
      created_at: "now",
    })
    expect(completed[0]).toMatchObject({
      state: "done",
      analysis_model: "gpt-5.6-terra",
      reasoning_effort: "medium",
      status_note: "由 gpt-5.6-terra · medium 完成",
    })
  })

  it("separates active imports from recent terminal failures", () => {
    const active = task({ id: "active", state: "analyzing", created_at: "2026-07-19T01:00:00Z" })
    const needsInput = task({ id: "needs", state: "needs-input", created_at: "2026-07-19T02:00:00Z" })
    const failed = task({ id: "failed", state: "failed", error: "下载失败", created_at: "2026-07-19T03:00:00Z" })
    const cancelled = task({ id: "cancelled", state: "cancelled", created_at: "2026-07-19T04:00:00Z" })
    const done = task({ id: "done", state: "done", paper_id: "paper-1" })
    const question = task({ id: "question", kind: "question" })
    expect(groupIntakeTasks([done, failed, question, active, cancelled, needsInput])).toEqual({
      active: [needsInput, active],
      failed: [cancelled, failed],
    })
  })
})

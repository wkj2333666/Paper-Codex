import { describe, expect, test } from "vitest"
import { initialState, reduceEvent, projectPaperCount } from "./state"
import type { Dashboard, StreamEvent } from "./types"

test("SSE stage and answer events update activity without polling", () => {
  const staged = reduceEvent(initialState, { id:1,type:"stage",task_id:"t1",payload:{state:"analyzing"},created_at:"now" })
  expect(staged.activities[0].label).toBe("正在分析论文")
  const answered = reduceEvent(staged, { id:2,type:"answer",task_id:"t1",payload:{text:"Evidence-backed answer"},created_at:"now" })
  expect(answered.latestAnswer).toBe("Evidence-backed answer")
})

describe("project paper counts", () => {
  test("counts one canonical paper in each folder membership", () => {
    const dashboard = {papers:[{id:"p1"}],projects:[{id:"a"},{id:"b"}],project_memberships:{a:["p1"],b:["p1"]}} as unknown as Dashboard
    expect(projectPaperCount(dashboard,"a")).toBe(1)
    expect(projectPaperCount(dashboard,"b")).toBe(1)
  })
})

test("unknown events remain visible for diagnostics", () => {
  const event = {id:3,type:"future-event",task_id:"t2",payload:{value:1},created_at:"now"} satisfies StreamEvent
  expect(reduceEvent(initialState,event).activities[0].label).toContain("future-event")
})

test("model fallback and discarded relations use readable activity labels", () => {
  const switched = reduceEvent(initialState, {
    id: 4,
    type: "model-switch",
    task_id: "t3",
    payload: { from: "gpt-5.6-sol", to: "gpt-5.6-terra" },
    created_at: "now",
  })
  expect(switched.activities[0].label).toBe(
    "模型容量不足：gpt-5.6-sol → gpt-5.6-terra",
  )
  const warned = reduceEvent(switched, {
    id: 5,
    type: "analysis-warning",
    task_id: "t3",
    payload: {
      source_key: "paper",
      relation_type: "reports",
      target_key: "method:missing",
    },
    created_at: "now",
  })
  expect(warned.activities[0].label).toBe(
    "已忽略无法定位的图谱关系：paper --reports--> method:missing",
  )
})

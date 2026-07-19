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

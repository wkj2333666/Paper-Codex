import { describe, expect, it } from "vitest"
import {
  conversationInitialState,
  conversationReducer,
  reduceConversationEvent,
  type ConversationState,
} from "./conversation-store"

const event = (id: number, type: string, payload: Record<string, unknown>) => ({
  id,
  type,
  conversation_id: "conversation-1",
  message_id: "a",
  payload,
  created_at: "2026-01-01T00:00:00Z",
})

const detail = (id: string, content: string) => ({
  conversation: {
    id,
    title: `对话 ${id}`,
    thread_id: null,
    status: "idle",
    model: "gpt-test",
    reasoning_effort: "medium",
    service_tier: null,
    archived_at: null,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  },
  scopes: [{
    conversation_id: id,
    scope_type: "project" as const,
    scope_id: "project-one",
    added_at: "2026-01-01T00:00:00Z",
  }],
  messages: [{
    id: `message-${id}`,
    conversation_id: id,
    role: "assistant" as const,
    content,
    turn_id: null,
    status: "completed" as const,
    error: null,
    research_mode: "auto" as const,
    tool_preferences: [],
    citations: [],
    candidate_citations: [],
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  }],
})

describe("conversation store", () => {
  it("keeps only the latest readable reasoning section", () => {
    let state = reduceConversationEvent(conversationInitialState, event(1, "work-summary-delta", {
      turn_id: "turn-1", item_id: "reasoning-1", summary_index: 0, text: "正在核对",
    }))
    state = reduceConversationEvent(state, event(2, "work-summary-delta", {
      turn_id: "turn-1", item_id: "reasoning-1", summary_index: 0, text: "论文证据",
    }))
    state = reduceConversationEvent(state, event(3, "work-summary-part", {
      turn_id: "turn-1", item_id: "reasoning-1", summary_index: 1,
    }))
    state = reduceConversationEvent(state, event(4, "work-summary-delta", {
      turn_id: "turn-1", item_id: "reasoning-1", summary_index: 1, text: "正在形成方向建议",
    }))

    expect(state.messages.a.worklog?.summaries).toEqual([
      { item_id: "reasoning-1", summary_index: 1, text: "正在形成方向建议" },
    ])
  })

  it("replaces an earlier commentary item while appending deltas from the current item", () => {
    let state = reduceConversationEvent(conversationInitialState, event(1, "work-summary-delta", {
      turn_id: "turn-1", item_id: "commentary-1", summary_index: 0, text: "先核验",
    }))
    state = reduceConversationEvent(state, event(2, "work-summary-delta", {
      turn_id: "turn-1", item_id: "commentary-1", summary_index: 0, text: "术语",
    }))
    expect(state.messages.a.worklog?.summaries).toEqual([
      { item_id: "commentary-1", summary_index: 0, text: "先核验术语" },
    ])
    state = reduceConversationEvent(state, event(3, "work-summary-delta", {
      turn_id: "turn-1", item_id: "commentary-2", summary_index: 0, text: "再检查证据",
    }))

    expect(state.messages.a.worklog?.summaries).toEqual([
      { item_id: "commentary-2", summary_index: 0, text: "再检查证据" },
    ])
  })

  it("retains plan state but does not retain tool-call rows for rendering", () => {
    let state = reduceConversationEvent(conversationInitialState, event(1, "plan-updated", {
      turn_id: "turn-1",
      explanation: "先定位再回答",
      plan: [{ step: "定位证据", status: "inProgress" }],
    }))
    state = reduceConversationEvent(state, event(2, "plan-updated", {
      turn_id: "turn-1",
      plan: [{ step: "定位证据", status: "completed" }, { step: "组织回答", status: "inProgress" }],
    }))
    state = reduceConversationEvent(state, event(3, "work-item-updated", {
      turn_id: "turn-1", item_id: "tool-1", item_type: "webSearch", label: "检索论文", status: "inProgress",
    }))
    state = reduceConversationEvent(state, event(4, "work-item-updated", {
      turn_id: "turn-1", item_id: "tool-1", item_type: "webSearch", label: "检索论文", status: "completed",
    }))

    expect(state.messages.a.worklog?.plan?.steps).toEqual([
      { step: "定位证据", status: "completed" },
      { step: "组织回答", status: "inProgress" },
    ])
    expect(Object.values(state.messages.a.worklog?.items ?? {})).toEqual([])
  })

  it("tracks thread goal updates even when the event has no message id", () => {
    const goalEvent = { ...event(1, "goal-updated", {
      thread_id: "thread-1", objective: "完成综述", status: "active",
      token_budget: 40000, tokens_used: 1200, time_used_seconds: 35,
    }), message_id: null }
    const state = reduceConversationEvent(conversationInitialState, goalEvent)
    expect(state.goal).toEqual({
      thread_id: "thread-1", objective: "完成综述", status: "active",
      token_budget: 40000, tokens_used: 1200, time_used_seconds: 35,
    })
  })

  it("keeps persisted worklog state when completion hydrates database messages", () => {
    let state = reduceConversationEvent(conversationInitialState, event(1, "work-summary-delta", {
      item_id: "reasoning-1", summary_index: 0, text: "已经完成证据核验",
    }))
    state = {...state,activeConversationId:"conversation-1"}
    const hydrated = conversationReducer(state, {type:"hydrate-detail",expectedConversationId:"conversation-1",detail:{
      conversation:{id:"conversation-1",title:"研究",thread_id:"thread-1",status:"idle",model:"gpt-test",reasoning_effort:"low",service_tier:null,archived_at:null,created_at:"",updated_at:""},
      scopes:[],messages:[{...state.messages.a,status:"completed",content:"最终回答",worklog:undefined}],
    }})
    expect(hydrated.messages.a.worklog?.summaries[0].text).toBe("已经完成证据核验")
    expect(hydrated.lastEventId).toBe(1)
  })

  it("keeps the active conversation's Codex settings when loading details", () => {
    const detail = {
      conversation: {
        id:"conversation-1", title:"设置", thread_id:null, status:"idle", model:"gpt-test",
        reasoning_effort:"high", service_tier:"priority", archived_at:null,
        created_at:"", updated_at:"",
      },
      scopes: [], messages: [],
    }
    const state = conversationReducer(conversationInitialState, {type:"detail", detail})
    expect(state.activeSettings).toEqual({model:"gpt-test", reasoning_effort:"high", service_tier:"priority"})
  })

  it("tracks safe live answer deltas without rendering structured JSON", () => {
    let state = conversationInitialState
    state = reduceConversationEvent(state, event(4, "answer-progress", { phase: "reading" }))
    state = reduceConversationEvent(state, event(5, "answer-delta", { text: "逐步回答" }))
    expect(state.messages.a.content).toBe("")
    expect(state.messages.a.live_content).toBe("逐步回答")
    expect(state.messages.a.progress_phase).toBe("answering")
    expect(state.lastEventId).toBe(5)
  })

  it("clears a partial live answer when automatic retry starts", () => {
    let state = reduceConversationEvent(conversationInitialState, event(4, "answer-delta", { text: "半截回答" }))
    state = reduceConversationEvent(state, event(5, "answer-retry", {
      attempt: 2, max_attempts: 3, label: "Codex 连接中断，正在自动重试（第 2/3 次）…",
    }))
    expect(state.messages.a.live_content).toBe("")
    expect(state.messages.a.worklog).toBeUndefined()
    expect(state.messages.a.progress_label).toContain("自动重试")
  })

  it("replaces the placeholder with the validated final answer", () => {
    let state = reduceConversationEvent(conversationInitialState, event(4, "answer-progress", { phase: "reasoning" }))
    state = reduceConversationEvent(state, event(5, "answer-completed", { answer_markdown: "最终回答", citations: [] }))
    expect(state.messages.a).toMatchObject({ content: "最终回答", status: "completed", citations: [] })
    expect(state.messages.a.progress_phase).toBeUndefined()
  })

  it("keeps the model answer visible when answer validation fails", () => {
    const state = reduceConversationEvent(
      conversationInitialState,
      event(5, "answer-failed", {
        message: "candidate citation source URL does not match inspected evidence",
        answer_markdown: "AnyGrasp 使用稀疏三维编码器。",
      }),
    )
    expect(state.messages.a).toMatchObject({
      content: "AnyGrasp 使用稀疏三维编码器。",
      status: "failed",
    })
    expect(state.messages.a.error).toContain("source URL")
  })

  it("keeps external candidate citations separate on completion", () => {
    const candidate = {
      id:"candidate-1",
      work_id:"work/one",
      title:"Rule Complexity",
      source_url:"https://example.test/work",
      evidence_level:"abstract",
      quote:"Rules use short descriptions.",
      explanation:"支持规则描述复杂度",
    }
    const state = reduceConversationEvent(
      conversationInitialState,
      event(6,"answer-completed",{
        answer_markdown:"候选回答 [candidate-1]",
        citations:[],
        candidate_citations:[candidate],
      }),
    )
    expect(state.messages.a.citations).toEqual([])
    expect(state.messages.a.candidate_citations).toEqual([candidate])
  })

  it("records the Skill used by a newly streamed user message", () => {
    const state = reduceConversationEvent(
      conversationInitialState,
      event(6, "message-created", {
        role: "user",
        content: "分析实验设计",
        skill: { name: "paper-research" },
      }),
    )
    expect(state.messages.a).toMatchObject({
      role: "user",
      content: "分析实验设计",
      skill_name: "paper-research",
    })
  })

  it("keeps research progress informational while the answer is running", () => {
    const state = reduceConversationEvent(
      conversationInitialState,
      event(7,"answer-progress",{
        phase:"research-partial",
        label:"部分检索来源暂不可用",
      }),
    )
    expect(state.messages.a.status).toBe("streaming")
    expect(state.messages.a.progress_phase).toBe("research-partial")
    expect(state.messages.a.progress_label).toBe("部分检索来源暂不可用")
  })

  it("keeps the active answer while the history drawer opens", () => {
    const seed: ConversationState = {
      ...conversationInitialState,
      activeConversationId: "conversation-1",
      drawerOpen: false,
    }
    const next = conversationReducer(seed, { type: "drawer", open: true })
    expect(next.activeConversationId).toBe(seed.activeConversationId)
    expect(next.drawerOpen).toBe(true)
  })

  it("keeps the displayed conversation when the same conversation is activated again", () => {
    const streamed = reduceConversationEvent(
      conversationInitialState,
      event(8, "answer-completed", {
        answer_markdown: "当前可见回答",
        citations: [],
      }),
    )
    const seed: ConversationState = {
      ...streamed,
      activeConversationId: "conversation-1",
      activeSettings: {
        model: "gpt-5.6-sol",
        reasoning_effort: "high",
        service_tier: "fast",
      },
      scopes: [{
        conversation_id: "conversation-1",
        scope_type: "paper",
        scope_id: "arxiv:2402.05099",
        added_at: "2026-01-01T00:00:00Z",
      }],
      drawerOpen: true,
    }

    const next = conversationReducer(seed, {
      type: "active",
      id: "conversation-1",
    })

    expect(next).toBe(seed)
    expect(next.messages.a.content).toBe("当前可见回答")
    expect(next.activeSettings?.model).toBe("gpt-5.6-sol")
    expect(next.lastEventId).toBe(8)
  })

  it("clears stale details when a different conversation is activated", () => {
    const streamed = reduceConversationEvent(
      conversationInitialState,
      event(8, "answer-completed", {
        answer_markdown: "旧对话回答",
        citations: [],
      }),
    )
    const seed: ConversationState = {
      ...streamed,
      activeConversationId: "conversation-1",
      activeSettings: {
        model: "gpt-5.6-sol",
        reasoning_effort: "high",
        service_tier: null,
      },
    }

    const next = conversationReducer(seed, {
      type: "active",
      id: "conversation-2",
    })

    expect(next.activeConversationId).toBe("conversation-2")
    expect(next.activeSettings).toBeNull()
    expect(next.messages).toEqual({})
    expect(next.messageOrder).toEqual([])
    expect(next.lastEventId).toBe(0)
  })

  it("keeps the visible conversation while a history switch is loading", () => {
    const seed = conversationReducer(conversationInitialState, {
      type: "detail",
      detail: detail("conversation-1", "当前仍应可见"),
    })

    const next = conversationReducer(seed, {
      type: "switch-start",
      requestId: 1,
      conversationId: "conversation-2",
    })

    expect(next.activeConversationId).toBe("conversation-1")
    expect(next.messageOrder).toEqual(seed.messageOrder)
    expect(next.messages["message-conversation-1"].content).toBe("当前仍应可见")
    expect(next.pendingSwitch).toMatchObject({
      requestId: 1,
      conversationId: "conversation-2",
      status: "loading",
    })
  })

  it("ignores a stale history response and atomically installs the latest one", () => {
    const seed = conversationReducer(conversationInitialState, {
      type: "detail",
      detail: detail("conversation-1", "原对话"),
    })
    const first = conversationReducer(seed, {
      type: "switch-start",
      requestId: 1,
      conversationId: "conversation-2",
    })
    const second = conversationReducer(first, {
      type: "switch-start",
      requestId: 2,
      conversationId: "conversation-3",
    })

    const stale = conversationReducer(second, {
      type: "switch-resolved",
      requestId: 1,
      detail: detail("conversation-2", "不应出现"),
      targetSelection: { kind: "project" as const, id: "project-one" },
    })
    expect(stale).toBe(second)

    const resolved = conversationReducer(stale, {
      type: "switch-resolved",
      requestId: 2,
      detail: detail("conversation-3", "目标对话"),
      targetSelection: { kind: "project" as const, id: "project-one" },
    })
    expect(resolved.activeConversationId).toBe("conversation-3")
    expect(resolved.messageOrder).toEqual(["message-conversation-3"])
    expect(resolved.messages["message-conversation-3"].content).toBe("目标对话")
    expect(resolved.pendingSwitch).toMatchObject({
      requestId: 2,
      conversationId: "conversation-3",
      status: "resolved",
    })
  })

  it("keeps the original conversation when the latest history load fails", () => {
    const seed = conversationReducer(conversationInitialState, {
      type: "detail",
      detail: detail("conversation-1", "加载失败后保留"),
    })
    const started = conversationReducer(seed, {
      type: "switch-start",
      requestId: 4,
      conversationId: "conversation-2",
    })
    const failed = conversationReducer(started, {
      type: "switch-failed",
      requestId: 4,
    })

    expect(failed.pendingSwitch).toBeNull()
    expect(failed.activeConversationId).toBe("conversation-1")
    expect(failed.messages["message-conversation-1"].content).toBe("加载失败后保留")
  })

  it("does not let an ordinary stale detail response overwrite the active conversation", () => {
    const seed = conversationReducer(conversationInitialState, {
      type: "detail",
      detail: detail("conversation-2", "新对话"),
    })
    const next = conversationReducer(seed, {
      type: "hydrate-detail",
      expectedConversationId: "conversation-1",
      detail: detail("conversation-1", "过期响应"),
    })

    expect(next).toBe(seed)
    expect(next.messages["message-conversation-2"].content).toBe("新对话")
  })

  it("ignores a late stream event from the conversation that was switched away", () => {
    const seed = conversationReducer(conversationInitialState, {
      type: "detail",
      detail: detail("conversation-2", "当前对话"),
    })

    const next = reduceConversationEvent(
      seed,
      event(9, "answer-delta", { text: "旧对话迟到内容" }),
    )

    expect(next).toBe(seed)
    expect(next.messages.a).toBeUndefined()
  })

  it("rejects a switch response whose detail belongs to another conversation", () => {
    const seed = conversationReducer(conversationInitialState, {
      type: "detail",
      detail: detail("conversation-1", "原对话"),
    })
    const started = conversationReducer(seed, {
      type: "switch-start",
      requestId: 8,
      conversationId: "conversation-2",
    })
    const mismatched = conversationReducer(started, {
      type: "switch-resolved",
      requestId: 8,
      detail: detail("conversation-3", "错误目标"),
      targetSelection: { kind: "project" as const, id: "project-one" },
    })

    expect(mismatched).toBe(started)
    expect(mismatched.activeConversationId).toBe("conversation-1")
  })
})

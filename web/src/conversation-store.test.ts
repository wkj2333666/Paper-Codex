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

describe("conversation store", () => {
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

  it("replaces the placeholder with the validated final answer", () => {
    let state = reduceConversationEvent(conversationInitialState, event(4, "answer-progress", { phase: "reasoning" }))
    state = reduceConversationEvent(state, event(5, "answer-completed", { answer_markdown: "最终回答", citations: [] }))
    expect(state.messages.a).toMatchObject({ content: "最终回答", status: "completed", citations: [] })
    expect(state.messages.a.progress_phase).toBeUndefined()
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
})

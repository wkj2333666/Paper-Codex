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
  it("tracks semantic progress without rendering structured deltas", () => {
    let state = conversationInitialState
    state = reduceConversationEvent(state, event(4, "answer-progress", { phase: "reading" }))
    state = reduceConversationEvent(state, event(5, "answer-delta", { delta: '{"answer_markdown":' }))
    expect(state.messages.a.content).toBe("")
    expect(state.messages.a.progress_phase).toBe("reading")
    expect(state.lastEventId).toBe(5)
  })

  it("replaces the placeholder with the validated final answer", () => {
    let state = reduceConversationEvent(conversationInitialState, event(4, "answer-progress", { phase: "reasoning" }))
    state = reduceConversationEvent(state, event(5, "answer-completed", { answer_markdown: "最终回答", citations: [] }))
    expect(state.messages.a).toMatchObject({ content: "最终回答", status: "completed", citations: [] })
    expect(state.messages.a.progress_phase).toBeUndefined()
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

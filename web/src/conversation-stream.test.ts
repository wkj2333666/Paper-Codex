import { describe, expect, it } from "vitest"
import { maintainConversationEventStream, type ConversationStreamStatus } from "./conversation-stream"
import type { ConversationStreamEvent } from "./types"

const streamedEvent = (id: number): ConversationStreamEvent => ({
  id,
  type: "answer-delta",
  conversation_id: "conversation-1",
  message_id: "message-1",
  payload: { text: "增量" },
  created_at: "2026-08-23T13:24:50Z",
})

describe("conversation event stream recovery", () => {
  it("reports three reconnect attempts before exposing a terminal connection error", async () => {
    const statuses: ConversationStreamStatus[] = []
    let connections = 0

    await maintainConversationEventStream({
      conversationId: "conversation-1",
      after: 0,
      signal: new AbortController().signal,
      onEvent: () => undefined,
      onStatus: status => statuses.push(status),
      syncDetail: async () => undefined,
      connect: async () => { connections += 1; throw new Error("offline") },
      wait: async () => undefined,
    })

    expect(connections).toBe(4)
    expect(statuses).toEqual([
      { kind: "reconnecting", attempt: 1, maxAttempts: 3 },
      { kind: "reconnecting", attempt: 2, maxAttempts: 3 },
      { kind: "reconnecting", attempt: 3, maxAttempts: 3 },
      { kind: "failed", message: "Codex 连接已中断，自动重连三次仍未恢复。" },
    ])
  })

  it("resumes from the latest received event after reconnecting", async () => {
    const controller = new AbortController()
    const cursors: number[] = []
    let connection = 0

    await maintainConversationEventStream({
      conversationId: "conversation-1",
      after: 4,
      signal: controller.signal,
      onEvent: () => undefined,
      onStatus: () => undefined,
      syncDetail: async () => undefined,
      wait: async () => undefined,
      connect: async (_id, after, onEvent, _signal, onOpen) => {
        cursors.push(after)
        connection += 1
        await onOpen?.()
        if (connection === 1) {
          onEvent(streamedEvent(8))
          throw new Error("broken pipe")
        }
        controller.abort()
      },
    })

    expect(cursors).toEqual([4, 8])
  })

  it("does not reset the retry budget for connections that immediately close", async () => {
    const statuses: ConversationStreamStatus[] = []
    let connections = 0

    await maintainConversationEventStream({
      conversationId: "conversation-1",
      after: 0,
      signal: new AbortController().signal,
      onEvent: () => undefined,
      onStatus: status => statuses.push(status),
      syncDetail: async () => undefined,
      connect: async (_id, _after, _onEvent, _signal, onOpen) => {
        connections += 1
        await onOpen?.()
      },
      wait: async () => undefined,
      now: () => 0,
    })

    expect(connections).toBe(4)
    expect(statuses.at(-1)).toEqual({
      kind: "failed",
      message: "Codex 连接已中断，自动重连三次仍未恢复。",
    })
  })
})

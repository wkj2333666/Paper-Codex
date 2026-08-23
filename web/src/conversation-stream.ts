import { streamConversationEvents } from "./api"
import type { ConversationStreamEvent } from "./types"

const RETRY_DELAYS_MS = [1_000, 2_000, 4_000] as const
const STABLE_CONNECTION_MS = 30_000

export type ConversationStreamStatus =
  | { kind: "connected" }
  | { kind: "reconnecting"; attempt: number; maxAttempts: number }
  | { kind: "failed"; message: string }

type ConversationEventConnector = (
  conversationId: string,
  after: number,
  onEvent: (event: ConversationStreamEvent) => void,
  signal: AbortSignal,
  onOpen?: () => void | Promise<void>,
) => Promise<void>

interface MaintainConversationEventStreamOptions {
  conversationId: string
  after: number
  signal: AbortSignal
  onEvent: (event: ConversationStreamEvent) => void
  onStatus: (status: ConversationStreamStatus) => void
  syncDetail: () => Promise<void>
  connect?: ConversationEventConnector
  wait?: (delayMs: number, signal: AbortSignal) => Promise<void>
  now?: () => number
}

function waitForRetry(delayMs: number, signal: AbortSignal): Promise<void> {
  return new Promise(resolve => {
    if (signal.aborted) {
      resolve()
      return
    }
    const timer = window.setTimeout(resolve, delayMs)
    signal.addEventListener("abort", () => {
      window.clearTimeout(timer)
      resolve()
    }, { once: true })
  })
}

export async function maintainConversationEventStream({
  conversationId,
  after,
  signal,
  onEvent,
  onStatus,
  syncDetail,
  connect = streamConversationEvents,
  wait = waitForRetry,
  now = Date.now,
}: MaintainConversationEventStreamOptions): Promise<void> {
  let cursor = after
  let failedAttempts = 0

  while (!signal.aborted) {
    let openedAt: number | null = null
    let receivedEvent = false
    try {
      await connect(
        conversationId,
        cursor,
        event => {
          cursor = Math.max(cursor, event.id)
          receivedEvent = true
          onEvent(event)
        },
        signal,
        async () => {
          openedAt = now()
          onStatus({ kind: "connected" })
          await syncDetail().catch(() => undefined)
        },
      )
    } catch {
      // A closed or failed stream follows the same bounded recovery path.
    }

    if (signal.aborted) return
    if (receivedEvent || (openedAt !== null && now() - openedAt >= STABLE_CONNECTION_MS)) {
      failedAttempts = 0
    }
    failedAttempts += 1
    await syncDetail().catch(() => undefined)

    if (failedAttempts > RETRY_DELAYS_MS.length) {
      onStatus({ kind: "failed", message: "Codex 连接已中断，自动重连三次仍未恢复。" })
      return
    }

    onStatus({
      kind: "reconnecting",
      attempt: failedAttempts,
      maxAttempts: RETRY_DELAYS_MS.length,
    })
    await wait(RETRY_DELAYS_MS[failedAttempts - 1], signal)
  }
}

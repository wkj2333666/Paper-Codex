import { describe, expect, it } from "vitest"
import { ConversationScrollController, isConversationAtBottom } from "./conversation-scroll"

function viewport(remaining: number) {
  const calls: ScrollToOptions[] = []
  const value = {
    scrollHeight: 1000,
    clientHeight: 400,
    scrollTop: 600 - remaining,
    scrollTo: (options: ScrollToOptions) => calls.push(options),
  }
  return { calls, value }
}

describe("conversation scroll controller", () => {
  it("uses a 24px bottom threshold", () => {
    expect(isConversationAtBottom(viewport(24).value)).toBe(true)
    expect(isConversationAtBottom(viewport(25).value)).toBe(false)
  })

  it("pauses after scrolling up and resumes at the bottom", () => {
    const fake = viewport(0)
    const controller = new ConversationScrollController(() => fake.value)

    controller.handleScroll()
    fake.value.scrollTop = 500
    controller.handleScroll()
    controller.followContent()
    expect(fake.calls).toHaveLength(0)

    fake.value.scrollTop = 600
    controller.handleScroll()
    controller.followContent()
    expect(fake.calls.at(-1)).toEqual({ top: 1000, behavior: "smooth" })
  })

  it("positions a loaded conversation immediately", () => {
    const fake = viewport(100)
    const controller = new ConversationScrollController(() => fake.value)

    controller.positionInitial()

    expect(fake.calls).toEqual([{ top: 1000, behavior: "auto" }])
    expect(controller.isPinned()).toBe(true)
  })

  it("resets follow mode for a newly loaded conversation", () => {
    const fake = viewport(100)
    const controller = new ConversationScrollController(() => fake.value)
    controller.handleScroll()
    expect(controller.isPinned()).toBe(false)

    controller.reset()
    controller.followContent()

    expect(controller.isPinned()).toBe(true)
    expect(fake.calls).toEqual([{ top: 1000, behavior: "smooth" }])
  })

  it("does not treat downward smooth-scroll progress as a user pause", () => {
    const fake = viewport(0)
    const controller = new ConversationScrollController(() => fake.value)
    controller.handleScroll()

    fake.value.scrollHeight = 1200
    controller.followContent()
    fake.value.scrollTop = 650
    controller.handleScroll()

    expect(controller.isPinned()).toBe(true)
  })
})

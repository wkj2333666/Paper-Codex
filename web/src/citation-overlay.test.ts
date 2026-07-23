import { describe, expect, it } from "vitest"
import { citationsForPaper, latestAnswerCitations } from "./citation-overlay"
import { defaultCardPreference, isCompactAnnotationGutter } from "./AnnotationGutter"
import type { ChatMessage, MessageCitation } from "./types"

const citation = (id: string, paper_id = "p1", page = 1): MessageCitation => ({
  id, message_id: "m2", paper_id, revision: "r1", page, section: null,
  locator: null, quote: id, prefix: "", suffix: "", explanation: id, match_status: "exact",
})

const message = (id: string, role: ChatMessage["role"], citations: MessageCitation[]): ChatMessage => ({
  id, conversation_id: "c1", role, content: id, turn_id: null, status: "completed",
  error: null, citations, created_at: id, updated_at: id,
})

describe("citation overlay selectors", () => {
  it("takes only the newest assistant answer and removes duplicate citations", () => {
    expect(latestAnswerCitations(
      { u: message("u", "user", []), a1: message("a1", "assistant", [citation("old")]), a2: message("a2", "assistant", [citation("new"), citation("new")]) },
      ["u", "a1", "a2"],
    ).map(item => item.id)).toEqual(["new"])
  })

  it("clears the overlay when the newest answer has no citations", () => {
    expect(latestAnswerCitations(
      { a1: message("a1", "assistant", [citation("old")]), a2: message("a2", "assistant", []) },
      ["a1", "a2"],
    )).toEqual([])
  })

  it("filters to the open paper and sorts by page then id", () => {
    expect(citationsForPaper([citation("b", "p1", 3), citation("a", "p1", 1), citation("x", "p2", 1)], "p1").map(item => item.id)).toEqual(["a", "b"])
  })

  it("starts every explanation card collapsed", () => {
    expect(defaultCardPreference).toMatchObject({ collapsed: true, hidden: false })
  })

  it("uses a compact rail when every explanation is collapsed or hidden", () => {
    const items = [citation("a"), citation("b")].map(citation => ({ citation, status: "exact" as const }))
    expect(isCompactAnnotationGutter(items, {})).toBe(true)
    expect(isCompactAnnotationGutter(items, { a: { ...defaultCardPreference, collapsed: false } })).toBe(false)
    expect(isCompactAnnotationGutter(items, { a: { ...defaultCardPreference, hidden: true } })).toBe(true)
  })
})

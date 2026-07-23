import { describe, expect, it } from "vitest"
import { matchCitationText } from "./citation-matcher"

describe("matchCitationText", () => {
  it("finds an exact normalized quote across text spans", () => {
    expect(matchCitationText({ quote: "attention is all you need", revision: "r1" }, ["Attention is", " all   you need."], "r1"))
      .toMatchObject({ status: "exact", spanIndexes: [0, 1], ranges: [{ spanIndex: 0 }, { spanIndex: 1 }] })
  })

  it("uses a unique punctuation-insensitive fallback", () => {
    expect(matchCitationText({ quote: "encoder–decoder architecture", revision: "r1" }, ["The encoder / decoder", " architecture is retained."], "r1"))
      .toMatchObject({ status: "fuzzy", spanIndexes: [0, 1], ranges: [{ spanIndex: 0 }, { spanIndex: 1 }] })
  })

  it("falls back to the cited page when text cannot be located", () => {
    expect(matchCitationText({ quote: "a missing sentence", revision: "r1" }, ["unrelated text"], "r1"))
      .toEqual({ status: "page-only", spanIndexes: [], ranges: [] })
  })

  it("never reuses coordinates from an older revision", () => {
    expect(matchCitationText({ quote: "same text", revision: "old" }, ["same text"], "new"))
      .toEqual({ status: "stale", spanIndexes: [], ranges: [] })
  })

  it("returns character ranges instead of only whole text spans", () => {
    expect(matchCitationText({ quote: "attention", revision: "r1" }, ["Read attention here"], "r1")).toMatchObject({
      status: "exact",
      ranges: [{ spanIndex: 0, start: 5, end: 14 }],
    })
  })

  it("returns no ranges for page-only and stale matches", () => {
    expect(matchCitationText({ quote: "missing sentence", revision: "r1" }, ["unrelated"], "r1").ranges).toEqual([])
    expect(matchCitationText({ quote: "unrelated", revision: "old" }, ["unrelated"], "new").ranges).toEqual([])
  })
})

import { describe, expect, it } from "vitest"
import {
  PAPER_READING_POSITION_KEY,
  loadPaperReadingPosition,
  parsePaperReadingPositions,
  savePaperReadingPosition,
} from "./paper-reading-position"

const position = {
  mode: "original",
  page: 12,
  xRatio: 0.25,
  yRatio: 0.75,
  zoom: 140,
} as const

describe("paper reading position", () => {
  it("loads and saves through a versioned storage key", () => {
    const writes: Array<[string, string]> = []
    const storage = {
      getItem: (key: string) => key === PAPER_READING_POSITION_KEY ? null : null,
      setItem: (key: string, value: string) => { writes.push([key, value]) },
    }

    savePaperReadingPosition("paper-1", position, storage)

    expect(writes).toEqual([[PAPER_READING_POSITION_KEY, JSON.stringify({ "paper-1": position })]])
  })

  it("round-trips one paper position", () => {
    const values = JSON.stringify({ "paper-1": position })
    const storage = {
      getItem: (key: string) => key === PAPER_READING_POSITION_KEY ? values : null,
      setItem: () => {},
    }

    expect(loadPaperReadingPosition("paper-1", storage)).toEqual(position)
  })

  it("ignores malformed positions instead of breaking the reader", () => {
    expect(parsePaperReadingPositions("not-json")).toEqual({})
    expect(parsePaperReadingPositions('{"paper":{"page":2}}')).toEqual({})
    expect(parsePaperReadingPositions(JSON.stringify({
      "paper-1": { ...position, page: 0 },
      "paper-2": { ...position, zoom: 0 },
      "paper-3": { ...position, mode: "smart" },
      "paper-4": { ...position, xRatio: 2 },
      "paper-5": position,
    }))).toEqual({ "paper-5": position })
  })
})

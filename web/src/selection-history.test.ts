import { describe, expect, it } from "vitest"
import {
  SELECTION_HISTORY_KEY,
  selectionFromHistoryState,
  selectionHistoryState,
} from "./selection-history"

describe("selection history", () => {
  it("wraps a selection in a versioned history entry", () => {
    const selection = { kind: "paper", id: "paper-1", projectId: "project-1" } as const

    expect(selectionHistoryState(selection)).toEqual({
      [SELECTION_HISTORY_KEY]: selection,
    })
  })

  it("restores a valid selection from browser history", () => {
    const selection = { kind: "project", id: "project-1" } as const

    expect(selectionFromHistoryState(selectionHistoryState(selection))).toEqual(selection)
  })

  it("rejects malformed or foreign history state", () => {
    expect(selectionFromHistoryState(null)).toBeNull()
    expect(selectionFromHistoryState({ [SELECTION_HISTORY_KEY]: { kind: "window" } })).toBeNull()
    expect(selectionFromHistoryState({ [SELECTION_HISTORY_KEY]: { kind: "paper", id: 7 } })).toBeNull()
    expect(selectionFromHistoryState({ [SELECTION_HISTORY_KEY]: { kind: "paper" } })).toBeNull()
    expect(selectionFromHistoryState({ other: true })).toBeNull()
  })
})

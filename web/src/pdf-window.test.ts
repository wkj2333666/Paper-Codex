import { describe, expect, it } from "vitest"
import { pageItemsForNumber, stableVisiblePageRange, visiblePageWindow } from "./pdf-window"

describe("visiblePageWindow", () => {
  it("adds bounded overscan around the visible pages", () => {
    expect(visiblePageWindow({ pageCount: 20, firstVisible: 8, lastVisible: 9, overscan: 2 }))
      .toEqual([6, 7, 8, 9, 10, 11])
    expect(visiblePageWindow({ pageCount: 3, firstVisible: 1, lastVisible: 1, overscan: 2 }))
      .toEqual([1, 2, 3])
  })
})

describe("stableVisiblePageRange", () => {
  it("keeps the current state object when scrolling within the same visible pages", () => {
    const current = { first: 4, last: 5 }

    expect(stableVisiblePageRange(current, { first: 4, last: 5 })).toBe(current)
  })

  it("returns the new range when scrolling changes the visible pages", () => {
    const current = { first: 4, last: 5 }
    const next = { first: 5, last: 6 }

    expect(stableVisiblePageRange(current, next)).toBe(next)
  })
})

describe("pageItemsForNumber", () => {
  it("reuses one empty array for pages without items", () => {
    const itemsByPage = new Map<number, string[]>()

    expect(pageItemsForNumber(itemsByPage, 3)).toBe(pageItemsForNumber(itemsByPage, 3))
  })

  it("returns the stored items for a populated page", () => {
    const items = ["citation"]
    const itemsByPage = new Map([[3, items]])

    expect(pageItemsForNumber(itemsByPage, 3)).toBe(items)
  })
})

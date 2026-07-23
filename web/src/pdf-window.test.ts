import { describe, expect, it } from "vitest"
import { visiblePageWindow } from "./pdf-window"

describe("visiblePageWindow", () => {
  it("adds bounded overscan around the visible pages", () => {
    expect(visiblePageWindow({ pageCount: 20, firstVisible: 8, lastVisible: 9, overscan: 2 }))
      .toEqual([6, 7, 8, 9, 10, 11])
    expect(visiblePageWindow({ pageCount: 3, firstVisible: 1, lastVisible: 1, overscan: 2 }))
      .toEqual([1, 2, 3])
  })
})

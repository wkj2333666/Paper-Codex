import { describe, expect, it } from "vitest"
import * as pdfWindow from "./pdf-window"

const { pageItemsForNumber, stableVisiblePageRange, visiblePageWindow } = pdfWindow

type Rect = { left: number; top: number; width: number; height: number }
type ZoomLayout = { viewport: Rect; pages: Array<{ page: number; rect: Rect }> }
type ZoomAnchor = { page: number; xRatio: number; yRatio: number; viewportX: number; viewportY: number }

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

describe("PDF zoom viewport", () => {
  it("captures the page coordinate currently at the viewport center", () => {
    const capturePdfZoomAnchor = (pdfWindow as unknown as {
      capturePdfZoomAnchor?: (layout: ZoomLayout) => ZoomAnchor | null
    }).capturePdfZoomAnchor
    expect(capturePdfZoomAnchor).toBeTypeOf("function")
    if (!capturePdfZoomAnchor) return

    expect(capturePdfZoomAnchor({
      viewport: { left: 0, top: 0, width: 1_000, height: 800 },
      pages: [
        { page: 4, rect: { left: 200, top: 100, width: 600, height: 800 } },
        { page: 5, rect: { left: 200, top: 920, width: 600, height: 800 } },
      ],
    })).toEqual({ page: 4, xRatio: 0.5, yRatio: 0.375, viewportX: 500, viewportY: 400 })
  })

  it("restores the anchor and returns the visible pages after scrolling", () => {
    const finishPdfZoom = (pdfWindow as unknown as {
      finishPdfZoom?: (
        anchor: ZoomAnchor | null,
        readLayout: () => ZoomLayout,
        scrollBy: (left: number, top: number) => void,
      ) => { first: number; last: number } | null
    }).finishPdfZoom
    expect(finishPdfZoom).toBeTypeOf("function")
    if (!finishPdfZoom) return

    let scrollLeft = 0
    let scrollTop = 0
    const readLayout = (): ZoomLayout => ({
      viewport: { left: 0, top: 0, width: 1_000, height: 800 },
      pages: [
        { page: 4, rect: { left: 50 - scrollLeft, top: 50 - scrollTop, width: 900, height: 1_200 } },
        { page: 5, rect: { left: 50 - scrollLeft, top: 850 - scrollTop, width: 900, height: 1_200 } },
      ],
    })

    const visible = finishPdfZoom(
      { page: 4, xRatio: 0.5, yRatio: 0.375, viewportX: 500, viewportY: 400 },
      readLayout,
      (left, top) => { scrollLeft += left; scrollTop += top },
    )

    expect({ scrollLeft, scrollTop }).toEqual({ scrollLeft: 0, scrollTop: 100 })
    expect(visible).toEqual({ first: 4, last: 5 })
  })
})

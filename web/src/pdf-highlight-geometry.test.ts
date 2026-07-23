import { describe, expect, it } from "vitest"
import { clientRectToPageRect, mergeHighlightRects, pdfTextRangeToPageRect, textLayerScaleStyle, textLayerViewportScale, textRangeClientRects, textRangeToClientRect } from "./pdf-highlight-geometry"

describe("clientRectToPageRect", () => {
  const page = { left: 100, top: 200, right: 900, bottom: 1200, width: 800, height: 1000 }

  it("clips a text rect to the PDF page coordinate system", () => {
    const value = clientRectToPageRect({ left: 80, top: 250, right: 980, bottom: 350, width: 900, height: 100 }, page)
    expect(value).not.toBeNull()
    expect(value?.left).toBe(0)
    expect(value?.top).toBeCloseTo(0.05)
    expect(value?.width).toBe(1)
    expect(value?.height).toBeCloseTo(0.1)
  })

  it("rejects empty or non-finite rectangles", () => {
    expect(clientRectToPageRect({ left: 100, top: 250, right: 100, bottom: 300, width: 0, height: 50 }, page)).toBeNull()
    expect(clientRectToPageRect({ left: Number.NaN, top: 250, right: 200, bottom: 300, width: 100, height: 50 }, page)).toBeNull()
  })

  it("maps a selected substring inside a full PDF.js span box", () => {
    expect(textRangeToClientRect("abcdefghij", 0, 3, { left: 100, top: 200, right: 900, bottom: 220, width: 800, height: 20 }, value => value.length)).toEqual({
      left: 100,
      top: 200,
      right: 340,
      bottom: 220,
      width: 240,
      height: 20,
    })
  })

  it("merges overlapping boxes from adjacent PDF.js text spans", () => {
    expect(mergeHighlightRects([
      { left: 0.1, top: 0.2, width: 0.35, height: 0.03 },
      { left: 0.42, top: 0.201, width: 0.2, height: 0.03 },
    ])).toEqual([{ left: 0.1, top: 0.2, width: 0.52, height: 0.031 }])
  })

  it("does not merge stacked lines even when their boxes overlap vertically", () => {
    expect(mergeHighlightRects([
      { left: 0.1, top: 0.2, width: 0.35, height: 0.08 },
      { left: 0.1, top: 0.24, width: 0.2, height: 0.08 },
    ])).toHaveLength(2)
  })

  it("keeps the PDF.js text layer scale explicit", () => {
    expect(textLayerScaleStyle(1.35)).toBe("1.35")
    expect(textLayerScaleStyle(0)).toBe("1")
  })

  it("removes the device pixel ratio from PDF.js internal text layout scale", () => {
    expect(textLayerViewportScale(1.35, 1.25)).toBeCloseTo(1.08)
    expect(textLayerViewportScale(1.35, 1)).toBeCloseTo(1.35)
    expect(textLayerViewportScale(1.35, 0)).toBeCloseTo(1.35)
  })

  it("preserves the browser's exact rectangles for a selected text range", () => {
    const range = {
      getClientRects: () => [
        { left: 10, top: 20, right: 110, bottom: 32, width: 100, height: 12 },
        { left: 10, top: 34, right: 55, bottom: 46, width: 45, height: 12 },
      ],
    }
    expect(textRangeClientRects(range)).toEqual([
      { left: 10, top: 20, right: 110, bottom: 32, width: 100, height: 12 },
      { left: 10, top: 34, right: 55, bottom: 46, width: 45, height: 12 },
    ])
  })

  it("maps a PDF.js text item range without relying on DOM wrapping", () => {
    expect(pdfTextRangeToPageRect({
      str: "abcdefghij",
      transform: [10, 0, 0, 10, 100, 900],
      width: 500,
      height: 10,
    }, 1000, 1000, 0, 5)).toEqual({
      left: 0.1,
      top: 0.09,
      width: 0.25,
      height: 0.01,
    })
  })
})

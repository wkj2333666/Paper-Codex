import { describe, expect, it } from "vitest"
import { mapPdfColor, PdfDarkBlender, recolorBinaryPdfImage } from "./pdf-dark-blender"

const darkTheme = { background: "#171b19", foreground: "#edf2ed" }

describe("PDF dark-mode color mapping", () => {
  it("maps neutral vector endpoints to the configured reading colors", () => {
    expect(mapPdfColor("#000000", darkTheme)).toBe("#edf2ed")
    expect(mapPdfColor("#ffffff", darkTheme)).toBe("#171b19")
  })

  it("preserves chromatic vector colors", () => {
    expect(mapPdfColor("#d22730", darkTheme)).toBe("#d22730")
  })
})

describe("PDF dark-mode raster handling", () => {
  it("leaves ordinary color images byte-for-byte unchanged", () => {
    const data = new Uint8ClampedArray([
      210, 30, 40, 255,
      25, 170, 80, 255,
      35, 90, 220, 255,
      245, 245, 245, 255,
    ])
    const original = data.slice()

    expect(recolorBinaryPdfImage(data, darkTheme)).toBe(false)
    expect(data).toEqual(original)
  })

  it("maps binary neutral formula images to the reading colors", () => {
    const data = new Uint8ClampedArray([
      0, 0, 0, 255,
      255, 255, 255, 255,
      0, 0, 0, 96,
      255, 255, 255, 0,
    ])

    expect(recolorBinaryPdfImage(data, darkTheme)).toBe(true)
    expect(Array.from(data)).toEqual([
      237, 242, 237, 255,
      23, 27, 25, 255,
      237, 242, 237, 96,
      23, 27, 25, 0,
    ])
  })
})

class RecordingCanvasContext {
  #fillStyle = "#000000"
  #strokeStyle = "#000000"
  #savedFillStyles: string[] = []
  readonly renderedTextColors: string[] = []
  canvas = { width: 100, height: 100 }
  sampledBackground = new Uint8ClampedArray([23, 27, 25, 255])

  get fillStyle() { return this.#fillStyle }
  set fillStyle(value: string) { this.#fillStyle = value }
  get strokeStyle() { return this.#strokeStyle }
  set strokeStyle(value: string) { this.#strokeStyle = value }
  drawImage(..._args: unknown[]) {}
  fillText(..._args: unknown[]) { this.renderedTextColors.push(this.#fillStyle) }
  measureText(..._args: unknown[]) { return { width: 20, actualBoundingBoxAscent: 8, actualBoundingBoxDescent: 2 } }
  getImageData(..._args: unknown[]) { return { data: this.sampledBackground } }
  getTransform() { return { transformPoint: ({ x, y }: { x: number; y: number }) => ({ x, y }) } }
  save() { this.#savedFillStyles.push(this.#fillStyle) }
  restore() { this.#fillStyle = this.#savedFillStyles.pop() ?? this.#fillStyle }
}

describe("PDF dark-mode canvas interception", () => {
  it("restores the original context behavior when rendering ends", () => {
    const context = new RecordingCanvasContext()
    const blender = new PdfDarkBlender(context as unknown as CanvasRenderingContext2D, darkTheme)

    context.fillStyle = "#000000"
    context.strokeStyle = "#ffffff"
    expect(context.fillStyle).toBe("#edf2ed")
    expect(context.strokeStyle).toBe("#171b19")

    blender.unwrap()
    context.fillStyle = "#000000"
    context.strokeStyle = "#ffffff"
    expect(context.fillStyle).toBe("#000000")
    expect(context.strokeStyle).toBe("#ffffff")
  })

  it("keeps readable original text dark when its colored background is preserved", () => {
    const context = new RecordingCanvasContext()
    context.sampledBackground = new Uint8ClampedArray([232, 231, 255, 255])
    const blender = new PdfDarkBlender(context as unknown as CanvasRenderingContext2D, darkTheme)

    context.fillStyle = "#000000"
    context.fillText("Current ML systems", 10, 20)

    expect(context.renderedTextColors).toEqual(["#000000"])
    blender.unwrap()
  })

  it("still maps dark body text to the light foreground on the themed page background", () => {
    const context = new RecordingCanvasContext()
    const blender = new PdfDarkBlender(context as unknown as CanvasRenderingContext2D, darkTheme)

    context.fillStyle = "#000000"
    context.fillText("ordinary body text", 10, 20)

    expect(context.renderedTextColors).toEqual(["#edf2ed"])
    blender.unwrap()
  })
})

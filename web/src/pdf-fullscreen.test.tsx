// @ts-expect-error Node built-ins are available in Vitest
import { readFileSync } from "node:fs"
import { createElement, type ComponentType } from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"
import * as fullscreenControls from "./PdfFullscreenButton"

const { PdfFullscreenButton, togglePdfFullscreen } = fullscreenControls
const pdfReaderStyles = readFileSync(new URL("./pdf-reader.css", import.meta.url), "utf8").replace(/\s+/g, " ")

type ReaderToolbarProps = {
  fullscreen: boolean
  zoom: number
  onToggle: () => void
  onZoomChange: (zoom: number) => void
}

describe("PDF fullscreen controls", () => {
  it("exposes the current fullscreen state and action accessibly", () => {
    const enter = renderToStaticMarkup(<PdfFullscreenButton fullscreen={false} onToggle={() => {}} />)
    const exit = renderToStaticMarkup(<PdfFullscreenButton fullscreen onToggle={() => {}} />)

    expect(enter).toContain('aria-label="全屏阅读论文"')
    expect(enter).toContain('aria-pressed="false"')
    expect(exit).toContain('aria-label="退出全屏阅读"')
    expect(exit).toContain('aria-pressed="true"')
  })

  it("enters and exits fullscreen through the browser API", async () => {
    const requestFullscreen = vi.fn(async () => {})
    const exitFullscreen = vi.fn(async () => {})
    const element = { requestFullscreen } as unknown as HTMLElement
    const documentState = { fullscreenElement: null, exitFullscreen } as unknown as Document

    await togglePdfFullscreen(element, documentState)
    expect(requestFullscreen).toHaveBeenCalledOnce()
    expect(exitFullscreen).not.toHaveBeenCalled()

    Object.defineProperty(documentState, "fullscreenElement", { value: element })
    await togglePdfFullscreen(element, documentState)
    expect(exitFullscreen).toHaveBeenCalledOnce()
  })

  it("shows an accessible 75–200% zoom control only while reading fullscreen", () => {
    const ReaderToolbar = (fullscreenControls as unknown as { PdfReaderToolbar?: ComponentType<ReaderToolbarProps> }).PdfReaderToolbar
    expect(ReaderToolbar).toBeTypeOf("function")
    if (!ReaderToolbar) return

    const fullscreen = renderToStaticMarkup(createElement(ReaderToolbar, { fullscreen: true, zoom: 100, onToggle: () => {}, onZoomChange: () => {} }))
    const embedded = renderToStaticMarkup(createElement(ReaderToolbar, { fullscreen: false, zoom: 100, onToggle: () => {}, onZoomChange: () => {} }))

    expect(fullscreen).toContain('aria-label="PDF 缩放"')
    expect(fullscreen).toContain('type="range"')
    expect(fullscreen).toContain('min="75"')
    expect(fullscreen).toContain('max="200"')
    expect(fullscreen).toContain('value="100"')
    expect(fullscreen).toContain('>100%</button>')
    expect(embedded).not.toContain('type="range"')
  })

  it("clamps zoom values to the supported fullscreen range", () => {
    const clampPdfZoom = (fullscreenControls as unknown as { clampPdfZoom?: (zoom: number) => number }).clampPdfZoom
    expect(clampPdfZoom).toBeTypeOf("function")
    if (!clampPdfZoom) return

    expect(clampPdfZoom(60)).toBe(75)
    expect(clampPdfZoom(137)).toBe(137)
    expect(clampPdfZoom(230)).toBe(200)
  })

  it("lets zoomed fullscreen pages exceed the viewport without flex shrinking", () => {
    expect(pdfReaderStyles).toMatch(/\.pdf-viewer:fullscreen \.pdf-page-row\s*\{[^}]*width:\s*max-content[^}]*max-width:\s*none/)
    expect(pdfReaderStyles).toMatch(/\.pdf-viewer:fullscreen \.pdf-page-shell\s*\{[^}]*flex:\s*none[^}]*max-width:\s*none/)
  })
})

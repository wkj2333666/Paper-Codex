import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"
import { PdfFullscreenButton, togglePdfFullscreen } from "./PdfFullscreenButton"

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
})

// @ts-expect-error Node built-ins are available in Vitest
import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"
import { patchPdfJsDarkMode } from "../pdfjs-dark-mode-plugin"

describe("PDF.js dark-mode build patch", () => {
  it("injects inline color, image, transparency-group, and cleanup hooks", () => {
    const source = readFileSync(new URL("../node_modules/pdfjs-dist/build/pdf.mjs", import.meta.url), "utf8")
    const patched = patchPdfJsDarkMode(source)

    expect(patched).toContain('import { PdfDarkBlender } from "virtual:paper-codex-pdf-dark-blender";')
    expect(patched).toContain("const pdfDarkColors = this.pageColors;")
    expect(patched).toContain("this.pageColors = null;")
    expect(patched).toContain("this.blender = new PdfDarkBlender(this.ctx, pdfDarkColors);")
    expect(patched).toContain("this.blender.interceptGroup(this.ctx);")
    expect(patched).toContain("this.blender.cleanupGroup(groupCtx);")
    expect(patched).toContain("this.blender.unwrap();")
  })

  it("fails loudly when the pinned PDF.js source anchors change", () => {
    expect(() => patchPdfJsDarkMode("class CanvasGraphics {}"))
      .toThrow(/PDF\.js 5\.6\.205 dark-mode patch anchor/)
  })
})

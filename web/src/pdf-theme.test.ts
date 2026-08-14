// @ts-expect-error Node built-ins are available in Vitest
import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"

const viewerSource = readFileSync(new URL("./PdfDocumentViewer.tsx", import.meta.url), "utf8")
const pdfCss = readFileSync(new URL("./pdf-reader.css", import.meta.url), "utf8")
const annotationCss = readFileSync(new URL("./annotation-overlay.css", import.meta.url), "utf8")
const viteConfig = readFileSync(new URL("../vite.config.ts", import.meta.url), "utf8")
const packageJson = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")) as { dependencies: Record<string, string> }

describe("dark reading surface", () => {
  it("scopes dark mode to the PDF viewer", () => {
    expect(viewerSource).toContain('theme === "dark" ? "dark-reader " : ""')
    expect(pdfCss).toContain(".pdf-viewer.dark-reader")
    expect(pdfCss).toContain(".pdf-viewer.dark-reader .pdf-page-shell")
    expect(pdfCss).toContain(".pdf-viewer.dark-reader .pdf-page-number")
  })

  it("uses one ordered PDF.js render with the dark-mode blender", () => {
    expect(viewerSource).toContain("pageColors")
    expect(viewerSource.match(/page\.render\(/g)).toHaveLength(1)
    expect(viewerSource).not.toContain("operationsFilter")
    expect(viewerSource).not.toContain("imageCanvas")
    expect(viteConfig).toContain("pdfjsDarkModePlugin()")
    expect(packageJson.dependencies["pdfjs-dist"]).toBe("5.6.205")
    expect(pdfCss).not.toContain(".pdf-viewer.dark-reader .pdf-page canvas{filter:")
  })

  it("keeps annotation cards readable on the dark reading surface", () => {
    expect(annotationCss).toContain('[data-theme="dark"] .annotation-card')
    expect(annotationCss).toContain('[data-theme="dark"] .annotation-body')
    expect(annotationCss).toContain('[data-theme="dark"] .citation-highlight')
    expect(annotationCss).toContain('[data-theme="dark"] .annotation-browser button')
  })
})

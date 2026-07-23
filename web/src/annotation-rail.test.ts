// @ts-expect-error Node built-ins are available in Vitest
import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"

const annotationOverlay = readFileSync(new URL("./annotation-overlay.css", import.meta.url), "utf8")

describe("collapsed annotation rail", () => {
  it("removes the compact gutter from document flow so the PDF remains centered", () => {
    expect(annotationOverlay).toMatch(/\.pdf-page-row\.has-annotation:has\(\.annotation-gutter\.compact\)\{[^}]*display:flex[^}]*position:relative/)
    expect(annotationOverlay).toMatch(/\.pdf-page-row\.has-annotation:has\(\.annotation-gutter\.compact\) \.annotation-gutter\.compact\{[^}]*position:absolute[^}]*left:calc\(100% - 22px\)/)
    expect(annotationOverlay).toMatch(/\.annotation-gutter\.compact\{[^}]*width:44px/)
  })
})

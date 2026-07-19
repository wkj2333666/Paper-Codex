import { describe, expect, test } from "vitest"
// @ts-expect-error The app intentionally has no Node type dependency; Vitest runs this file in Node.
import { readFileSync } from "node:fs"

const css = readFileSync(new URL("./redesign.css", import.meta.url), "utf8")

function rule(selector: string) {
  const start = css.indexOf(`${selector}{`)
  expect(start, `missing CSS rule ${selector}`).toBeGreaterThanOrEqual(0)
  const end = css.indexOf("}", start)
  return css.slice(start, end + 1)
}

describe("paper reading typography", () => {
  test("uses comfortable summary-card typography", () => {
    expect(rule(".brief-card")).toContain("padding:22px")
    expect(rule(".brief-card h3")).toContain("font-size:18px")
    expect(rule(".brief-card li")).toContain("font-size:16px")
    expect(rule(".brief-card li")).toContain("line-height:1.8")
  })

  test("uses readable long-form and supporting typography", () => {
    expect(rule(".analysis-panel p")).toContain("font-size:17px")
    expect(rule(".analysis-panel p")).toContain("line-height:1.85")
    expect(rule(".analysis-panel p")).toContain("max-width:72ch")
    expect(rule(".analysis-panel li")).toContain("font-size:16px")
    expect(rule(".evidence-list article")).toContain("font-size:14px")
    expect(rule(".paper-graph p")).toContain("font-size:14px")
  })

  test("keeps summary cards readable on narrow screens", () => {
    expect(css).toContain("@media(max-width:900px){.brief-grid{grid-template-columns:1fr}")
    expect(css).toContain(".brief-card{padding:18px}")
  })
})

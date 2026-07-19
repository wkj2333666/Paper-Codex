// @ts-expect-error Node built-ins are available in Vitest
import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"

const panelLayout = readFileSync(new URL("./panel-layout.css", import.meta.url), "utf8")

describe("collapsible workspace layout", () => {
  it("releases wide layout columns to 42px rails", () => {
    expect(panelLayout).toMatch(/--sidebar-width:\s*248px/)
    expect(panelLayout).toMatch(/sidebar-collapsed[^}]*--sidebar-width:\s*42px/)
    expect(panelLayout).toMatch(/codex-collapsed[^}]*--codex-width:\s*42px/)
    expect(panelLayout).toMatch(/paper-graph-collapsed[^}]*42px/)
  })

  it("uses fixed single overlay drawers below 1050px", () => {
    expect(panelLayout).toMatch(/@media\(max-width:1050px\)/)
    expect(panelLayout).toMatch(/workspace-panel[^}]*position:\s*fixed/)
    expect(panelLayout).toMatch(/drawer-open[^}]*transform:\s*translateX\(0\)/)
    expect(panelLayout).toMatch(/drawer-backdrop[^}]*position:\s*fixed/)
  })

  it("supports edge controls and reduced motion", () => {
    expect(panelLayout).toMatch(/\.panel-rail/)
    expect(panelLayout).toMatch(/prefers-reduced-motion:\s*reduce/)
  })
})

// @ts-expect-error Node built-ins are available in Vitest
import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"

const panelLayout = readFileSync(new URL("./panel-layout.css", import.meta.url), "utf8")
const panelControls = readFileSync(new URL("./PanelControls.tsx", import.meta.url), "utf8")

describe("collapsible workspace layout", () => {
  it("uses dedicated track widths so inline saved widths cannot override collapsed rails", () => {
    expect(panelLayout).toMatch(/--sidebar-track-width:\s*var\(--sidebar-width\)/)
    expect(panelLayout).toMatch(/--codex-track-width:\s*var\(--codex-width\)/)
    expect(panelLayout).toMatch(/sidebar-collapsed[^}]*--sidebar-track-width:\s*42px/)
    expect(panelLayout).toMatch(/codex-collapsed[^}]*--codex-track-width:\s*42px/)
    expect(panelLayout).toMatch(/grid-template-columns:\s*var\(--sidebar-track-width\)[^;]*var\(--codex-track-width\)/)
    expect(panelLayout).toMatch(/paper-graph-collapsed[^}]*42px/)
  })

  it("pins every desktop region to a stable grid column when optional dividers disappear", () => {
    expect(panelControls).toMatch(/data-panel=\{panel\}/)
    expect(panelLayout).toMatch(/\[data-panel="sidebar"\][^}]*grid-column:\s*1/)
    expect(panelLayout).toMatch(/resizable-divider-sidebar[^}]*grid-column:\s*2/)
    expect(panelLayout).toMatch(/\.app-shell\s*>\s*\.main-pane[^}]*grid-column:\s*3/)
    expect(panelLayout).toMatch(/resizable-divider-codex[^}]*grid-column:\s*4/)
    expect(panelLayout).toMatch(/\[data-panel="codex"\][^}]*grid-column:\s*5/)
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

import { describe, expect, it } from "vitest"
import {
  DEFAULT_PANEL_LAYOUT,
  parsePanelLayout,
  resetPanelWidth,
  resizePanel,
} from "./panel-preferences"

describe("panel size model", () => {
  it("clamps each panel to its declared range", () => {
    expect(resizePanel(DEFAULT_PANEL_LAYOUT, "sidebar", -500, 2200).widths.sidebar).toBe(180)
    expect(resizePanel(DEFAULT_PANEL_LAYOUT, "paperGraph", 900, 2200).widths.paperGraph).toBe(520)
    expect(resizePanel(DEFAULT_PANEL_LAYOUT, "codex", 900, 2200).widths.codex).toBe(640)
  })

  it("preserves the 520px reading surface", () => {
    const resized = resizePanel(DEFAULT_PANEL_LAYOUT, "codex", 300, 1600)
    expect(resized.widths.sidebar + resized.widths.paperGraph + resized.widths.codex).toBeLessThanOrEqual(1600 - 520 - 18)
  })

  it("ignores collapsed panels when protecting the reading surface", () => {
    const value = { ...DEFAULT_PANEL_LAYOUT, paperGraphOpen: false }
    const resized = resizePanel(value, "codex", 300, 1200)
    expect(resized.widths.codex).toBe(414)
  })

  it("migrates v1 visibility without losing collapsed state", () => {
    const value = parsePanelLayout(null, '{"sidebarOpen":false,"codexOpen":true,"paperGraphOpen":false}')
    expect(value.sidebarOpen).toBe(false)
    expect(value.widths).toEqual({ sidebar: 248, paperGraph: 340, codex: 380 })
  })

  it("resets one panel without changing the others", () => {
    const resized = resizePanel(DEFAULT_PANEL_LAYOUT, "sidebar", 80, 1600)
    expect(resetPanelWidth(resized, "sidebar").widths).toEqual(DEFAULT_PANEL_LAYOUT.widths)
  })
})

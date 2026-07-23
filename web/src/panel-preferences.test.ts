import { describe, expect, it } from "vitest"
import {
  DEFAULT_PANEL_VISIBILITY,
  DEFAULT_PANEL_LAYOUT,
  PANEL_LAYOUT_KEY,
  PANEL_VISIBILITY_KEY,
  loadPanelLayout,
  loadPanelVisibility,
  parsePanelLayout,
  parsePanelVisibility,
  savePanelLayout,
  savePanelVisibility,
  setPanelOpen,
} from "./panel-preferences"

describe("panel preferences", () => {
  it("uses open defaults when storage is empty or malformed", () => {
    expect(parsePanelVisibility(null)).toEqual(DEFAULT_PANEL_VISIBILITY)
    expect(parsePanelVisibility("{")).toEqual(DEFAULT_PANEL_VISIBILITY)
  })

  it("keeps valid fields and defaults invalid fields", () => {
    expect(parsePanelVisibility('{"sidebarOpen":false,"codexOpen":"no","paperGraphOpen":false}')).toEqual({
      sidebarOpen: false,
      codexOpen: true,
      paperGraphOpen: false,
    })
  })

  it("loads and saves through the versioned storage key", () => {
    const writes: Array<[string, string]> = []
    const storage = {
      getItem: (key: string) => key === PANEL_VISIBILITY_KEY ? '{"sidebarOpen":false}' : null,
      setItem: (key: string, value: string) => { writes.push([key, value]) },
    }

    expect(loadPanelVisibility(storage)).toEqual({ sidebarOpen: false, codexOpen: true, paperGraphOpen: true })
    savePanelVisibility({ sidebarOpen: false, codexOpen: true, paperGraphOpen: false }, storage)

    expect(writes).toEqual([[
      PANEL_VISIBILITY_KEY,
      '{"sidebarOpen":false,"codexOpen":true,"paperGraphOpen":false}',
    ]])
  })

  it("updates one panel immutably", () => {
    const next = setPanelOpen(DEFAULT_PANEL_VISIBILITY, "codex", false)

    expect(next).toEqual({ sidebarOpen: true, codexOpen: false, paperGraphOpen: true })
    expect(DEFAULT_PANEL_VISIBILITY.codexOpen).toBe(true)
  })

  it("loads v2 layout before falling back to the v1 visibility key", () => {
    const writes: Array<[string, string]> = []
    const storage = {
      getItem: (key: string) => {
        if (key === PANEL_LAYOUT_KEY) return '{"sidebarOpen":true,"codexOpen":false,"paperGraphOpen":true,"widths":{"sidebar":300,"paperGraph":360,"codex":410},"conversationDrawerWidth":260}'
        if (key === PANEL_VISIBILITY_KEY) return '{"sidebarOpen":false}'
        return null
      },
      setItem: (key: string, value: string) => { writes.push([key, value]) },
    }

    const layout = loadPanelLayout(storage)
    expect(layout.codexOpen).toBe(false)
    expect(layout.widths).toEqual({ sidebar: 300, paperGraph: 360, codex: 410 })

    savePanelLayout(layout, storage)
    expect(writes).toEqual([[PANEL_LAYOUT_KEY, JSON.stringify(layout)]])
  })

  it("defaults malformed v2 fields independently", () => {
    expect(parsePanelLayout('{"sidebarOpen":"bad","widths":{"sidebar":999,"paperGraph":300}}')).toEqual({
      ...DEFAULT_PANEL_LAYOUT,
      widths: { ...DEFAULT_PANEL_LAYOUT.widths, paperGraph: 300 },
    })
  })
})

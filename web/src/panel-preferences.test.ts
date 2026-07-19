import { describe, expect, it } from "vitest"
import {
  DEFAULT_PANEL_VISIBILITY,
  PANEL_VISIBILITY_KEY,
  loadPanelVisibility,
  parsePanelVisibility,
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
})

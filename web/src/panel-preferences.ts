export type PanelName = "sidebar" | "codex" | "paperGraph"

export interface PanelVisibility {
  sidebarOpen: boolean
  codexOpen: boolean
  paperGraphOpen: boolean
}

export const PANEL_VISIBILITY_KEY = "paper-codex:panel-visibility:v1"

export const DEFAULT_PANEL_VISIBILITY: PanelVisibility = {
  sidebarOpen: true,
  codexOpen: true,
  paperGraphOpen: true,
}

type ReadStorage = Pick<Storage, "getItem">
type WriteStorage = Pick<Storage, "setItem">

const fieldByPanel: Record<PanelName, keyof PanelVisibility> = {
  sidebar: "sidebarOpen",
  codex: "codexOpen",
  paperGraph: "paperGraphOpen",
}

export function parsePanelVisibility(raw: string | null): PanelVisibility {
  if (!raw) return { ...DEFAULT_PANEL_VISIBILITY }
  try {
    const value = JSON.parse(raw) as Partial<PanelVisibility>
    return {
      sidebarOpen: typeof value.sidebarOpen === "boolean" ? value.sidebarOpen : true,
      codexOpen: typeof value.codexOpen === "boolean" ? value.codexOpen : true,
      paperGraphOpen: typeof value.paperGraphOpen === "boolean" ? value.paperGraphOpen : true,
    }
  } catch {
    return { ...DEFAULT_PANEL_VISIBILITY }
  }
}

export function loadPanelVisibility(storage: ReadStorage = window.localStorage): PanelVisibility {
  try {
    return parsePanelVisibility(storage.getItem(PANEL_VISIBILITY_KEY))
  } catch {
    return { ...DEFAULT_PANEL_VISIBILITY }
  }
}

export function savePanelVisibility(value: PanelVisibility, storage: WriteStorage = window.localStorage): void {
  try {
    storage.setItem(PANEL_VISIBILITY_KEY, JSON.stringify(value))
  } catch {
    // Browsers may deny storage in private or restricted contexts.
  }
}

export function setPanelOpen(value: PanelVisibility, panel: PanelName, open: boolean): PanelVisibility {
  return { ...value, [fieldByPanel[panel]]: open }
}

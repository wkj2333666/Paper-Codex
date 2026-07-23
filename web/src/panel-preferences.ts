export type PanelName = "sidebar" | "codex" | "paperGraph"

export interface PanelVisibility {
  sidebarOpen: boolean
  codexOpen: boolean
  paperGraphOpen: boolean
}

export type PanelWidths = Record<PanelName, number>

export interface PanelLayout extends PanelVisibility {
  widths: PanelWidths
  conversationDrawerWidth: number
}

export const PANEL_VISIBILITY_KEY = "paper-codex:panel-visibility:v1"
export const PANEL_LAYOUT_KEY = "paper-codex:panel-layout:v2"

export const DEFAULT_PANEL_VISIBILITY: PanelVisibility = {
  sidebarOpen: true,
  codexOpen: true,
  paperGraphOpen: true,
}

export const PANEL_LIMITS = {
  sidebar: [180, 420],
  paperGraph: [280, 520],
  codex: [320, 640],
} as const satisfies Record<PanelName, readonly [number, number]>

export const DEFAULT_PANEL_LAYOUT: PanelLayout = {
  ...DEFAULT_PANEL_VISIBILITY,
  widths: { sidebar: 248, paperGraph: 340, codex: 380 },
  conversationDrawerWidth: 240,
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

export function setPanelOpen<T extends PanelVisibility>(value: T, panel: PanelName, open: boolean): T {
  return { ...value, [fieldByPanel[panel]]: open }
}

export function clampPanelWidth(panel: PanelName, width: number): number {
  const [min, max] = PANEL_LIMITS[panel]
  return Math.min(max, Math.max(min, Math.round(width)))
}

export function parsePanelLayout(rawV2: string | null, rawV1: string | null = null): PanelLayout {
  if (!rawV2) {
    return {
      ...DEFAULT_PANEL_LAYOUT,
      ...parsePanelVisibility(rawV1),
      widths: { ...DEFAULT_PANEL_LAYOUT.widths },
    }
  }

  try {
    const value = JSON.parse(rawV2) as Partial<PanelLayout> & { widths?: Partial<PanelWidths> }
    const visibility = parsePanelVisibility(rawV2)
    const widths = { ...DEFAULT_PANEL_LAYOUT.widths }
    for (const panel of Object.keys(widths) as PanelName[]) {
      const candidate = value.widths?.[panel]
      const [min, max] = PANEL_LIMITS[panel]
      if (typeof candidate === "number" && Number.isFinite(candidate) && candidate >= min && candidate <= max) {
        widths[panel] = Math.round(candidate)
      }
    }
    const drawer = value.conversationDrawerWidth
    return {
      ...visibility,
      widths,
      conversationDrawerWidth: typeof drawer === "number" && Number.isFinite(drawer)
        ? Math.min(360, Math.max(200, Math.round(drawer)))
        : DEFAULT_PANEL_LAYOUT.conversationDrawerWidth,
    }
  } catch {
    return {
      ...DEFAULT_PANEL_LAYOUT,
      widths: { ...DEFAULT_PANEL_LAYOUT.widths },
    }
  }
}

export function loadPanelLayout(storage: ReadStorage = window.localStorage): PanelLayout {
  try {
    return parsePanelLayout(storage.getItem(PANEL_LAYOUT_KEY), storage.getItem(PANEL_VISIBILITY_KEY))
  } catch {
    return { ...DEFAULT_PANEL_LAYOUT, widths: { ...DEFAULT_PANEL_LAYOUT.widths } }
  }
}

export function savePanelLayout(value: PanelLayout, storage: WriteStorage = window.localStorage): void {
  try {
    storage.setItem(PANEL_LAYOUT_KEY, JSON.stringify(value))
  } catch {
    // Browsers may deny storage in private or restricted contexts.
  }
}

export function resizePanel(value: PanelLayout, panel: PanelName, delta: number, viewport: number): PanelLayout {
  let nextWidth = clampPanelWidth(panel, value.widths[panel] + delta)
  if (value[fieldByPanel[panel]]) {
    const otherOpenWidth = (Object.keys(value.widths) as PanelName[])
      .filter(name => name !== panel && value[fieldByPanel[name]])
      .reduce((sum, name) => sum + value.widths[name], 0)
    const availableForTarget = viewport - 520 - 18 - otherOpenWidth
    nextWidth = clampPanelWidth(panel, Math.min(nextWidth, availableForTarget))
  }
  return { ...value, widths: { ...value.widths, [panel]: nextWidth } }
}

export function resetPanelWidth(value: PanelLayout, panel: PanelName): PanelLayout {
  return {
    ...value,
    widths: { ...value.widths, [panel]: DEFAULT_PANEL_LAYOUT.widths[panel] },
  }
}

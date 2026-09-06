export const PAPER_READING_POSITION_KEY = "paper-codex:reading-positions:v1"

export type PaperReadingPosition = {
  mode: "enhanced" | "original"
  page: number
  xRatio: number
  yRatio: number
  zoom: number
}

type ReadStorage = Pick<Storage, "getItem">
type WriteStorage = Pick<Storage, "setItem">

function isPaperReadingPosition(value: unknown): value is PaperReadingPosition {
  if (!value || typeof value !== "object") return false
  const position = value as Record<string, unknown>
  return position.mode === "enhanced" || position.mode === "original"
    ? typeof position.page === "number" && Number.isInteger(position.page) && position.page > 0
      && typeof position.xRatio === "number" && Number.isFinite(position.xRatio) && position.xRatio >= 0 && position.xRatio <= 1
      && typeof position.yRatio === "number" && Number.isFinite(position.yRatio) && position.yRatio >= 0 && position.yRatio <= 1
      && typeof position.zoom === "number" && Number.isFinite(position.zoom) && position.zoom >= 75 && position.zoom <= 200
    : false
}

export function parsePaperReadingPositions(value: string | null): Record<string, PaperReadingPosition> {
  if (!value) return {}
  try {
    const parsed = JSON.parse(value) as Record<string, unknown>
    if (!parsed || typeof parsed !== "object") return {}
    return Object.fromEntries(
      Object.entries(parsed).filter(([paperId, position]) =>
        paperId.length > 0 && isPaperReadingPosition(position),
      ) as Array<[string, PaperReadingPosition]>,
    )
  } catch {
    return {}
  }
}

export function loadPaperReadingPosition(
  paperId: string,
  storage: ReadStorage = window.localStorage,
): PaperReadingPosition | null {
  try {
    return parsePaperReadingPositions(storage.getItem(PAPER_READING_POSITION_KEY))[paperId] ?? null
  } catch {
    return null
  }
}

export function savePaperReadingPosition(
  paperId: string,
  position: PaperReadingPosition,
  storage: WriteStorage = window.localStorage,
): void {
  try {
    const current = parsePaperReadingPositions(storage.getItem(PAPER_READING_POSITION_KEY))
    storage.setItem(PAPER_READING_POSITION_KEY, JSON.stringify({ ...current, [paperId]: position }))
  } catch {
    // Browsers may deny storage in private or restricted contexts.
  }
}

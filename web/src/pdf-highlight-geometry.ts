export interface ClientRectLike {
  left: number
  top: number
  right: number
  bottom: number
  width: number
  height: number
}

export interface PageRectLike {
  left: number
  top: number
  right: number
  bottom: number
  width: number
  height: number
}

export interface PageHighlightRect {
  left: number
  top: number
  width: number
  height: number
}

export interface PdfTextItemLike {
  str: string
  transform: readonly number[]
  width: number
  height: number
}

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value))

export function textLayerScaleStyle(scale: number): string {
  return Number.isFinite(scale) && scale > 0 ? String(scale) : "1"
}

export function textLayerViewportScale(scale: number, pixelRatio: number): number {
  if (!Number.isFinite(scale) || scale <= 0) return 1
  return Number.isFinite(pixelRatio) && pixelRatio > 0 ? scale / pixelRatio : scale
}

export function pdfTextRangeToPageRect(item: PdfTextItemLike, pageWidth: number, pageHeight: number, start: number, end: number, measure?: (value: string) => number): PageHighlightRect | null {
  const [a, b, c, d, e, f] = item.transform
  if (![pageWidth, pageHeight, a, b, c, d, e, f, item.width, item.height].every(Number.isFinite) || pageWidth <= 0 || pageHeight <= 0 || item.width <= 0) return null
  const safeStart = clamp(Math.floor(start), 0, item.str.length)
  const safeEnd = clamp(Math.ceil(end), safeStart, item.str.length)
  if (safeStart >= safeEnd) return null
  const total = measure?.(item.str) ?? item.str.length
  const prefix = measure?.(item.str.slice(0, safeStart)) ?? safeStart
  const selectedEnd = measure?.(item.str.slice(0, safeEnd)) ?? safeEnd
  if (![total, prefix, selectedEnd].every(Number.isFinite) || total <= 0 || selectedEnd <= prefix) return null
  const leftRatio = clamp(prefix / total, 0, 1)
  const rightRatio = clamp(selectedEnd / total, 0, 1)
  const fontHeight = Math.max(item.height, Math.hypot(c, d))
  const top = pageHeight - f - fontHeight
  if (top < -fontHeight || top > pageHeight) return null
  return {
    left: clamp((e + item.width * leftRatio) / pageWidth, 0, 1),
    top: clamp(top / pageHeight, 0, 1),
    width: clamp((item.width * (rightRatio - leftRatio)) / pageWidth, 0, 1),
    height: clamp(fontHeight / pageHeight, 0, 1),
  }
}

export function textRangeClientRects(range: { getClientRects: () => ArrayLike<ClientRectLike> }): ClientRectLike[] {
  return Array.from(range.getClientRects())
    .filter(rect => [rect.left, rect.top, rect.right, rect.bottom, rect.width, rect.height].every(Number.isFinite) && rect.right > rect.left && rect.bottom > rect.top)
    .map(rect => ({ left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom, width: rect.width, height: rect.height }))
}

export function mergeHighlightRects(rects: PageHighlightRect[]): PageHighlightRect[] {
  const sorted = rects
    .filter(rect => [rect.left, rect.top, rect.width, rect.height].every(Number.isFinite) && rect.width > 0 && rect.height > 0)
    .map(rect => ({ ...rect }))
    .sort((a, b) => a.top - b.top || a.left - b.left)
  const merged: PageHighlightRect[] = []
  for (const next of sorted) {
    const current = merged.at(-1)
    if (!current) {
      merged.push(next)
      continue
    }
    const currentRight = current.left + current.width
    const nextRight = next.left + next.width
    const currentBottom = current.top + current.height
    const nextBottom = next.top + next.height
    const topDelta = Math.abs(next.top - current.top)
    const sameLine = topDelta <= Math.min(current.height, next.height) * 0.2
    const touches = next.left <= currentRight + 0.006
    if (sameLine && touches) {
      const left = Math.min(current.left, next.left)
      const top = Math.min(current.top, next.top)
      const right = Math.max(currentRight, nextRight)
      const bottom = Math.max(currentBottom, nextBottom)
      merged[merged.length - 1] = { left, top, width: right - left, height: bottom - top }
    } else {
      merged.push(next)
    }
  }
  return merged
}

export function textRangeToClientRect(source: string, start: number, end: number, span: ClientRectLike, measure: (value: string) => number): ClientRectLike | null {
  if (![span.left, span.top, span.right, span.bottom, span.width, span.height].every(Number.isFinite) || span.width <= 0 || span.height <= 0) return null
  const safeStart = clamp(Math.floor(start), 0, source.length)
  const safeEnd = clamp(Math.ceil(end), safeStart, source.length)
  if (safeStart >= safeEnd) return null
  const total = measure(source)
  const prefix = measure(source.slice(0, safeStart))
  const selectedEnd = measure(source.slice(0, safeEnd))
  if (![total, prefix, selectedEnd].every(Number.isFinite) || total <= 0 || selectedEnd <= prefix) return null
  const left = span.left + span.width * clamp(prefix / total, 0, 1)
  const right = span.left + span.width * clamp(selectedEnd / total, 0, 1)
  if (right <= left) return null
  return { left, top: span.top, right, bottom: span.bottom, width: right - left, height: span.height }
}

export function clientRectToPageRect(rect: ClientRectLike, page: PageRectLike): PageHighlightRect | null {
  if (![rect.left, rect.top, rect.right, rect.bottom, rect.width, rect.height, page.left, page.top, page.right, page.bottom, page.width, page.height].every(Number.isFinite)) return null
  if (rect.width <= 0 || rect.height <= 0 || page.width <= 0 || page.height <= 0) return null
  const left = clamp((Math.max(rect.left, page.left) - page.left) / page.width, 0, 1)
  const top = clamp((Math.max(rect.top, page.top) - page.top) / page.height, 0, 1)
  const right = clamp((Math.min(rect.right, page.right) - page.left) / page.width, 0, 1)
  const bottom = clamp((Math.min(rect.bottom, page.bottom) - page.top) / page.height, 0, 1)
  if (right <= left || bottom <= top) return null
  return { left, top, width: right - left, height: bottom - top }
}

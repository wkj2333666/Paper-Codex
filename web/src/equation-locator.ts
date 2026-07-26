import { matchCitationText, type CitationLocator } from "./citation-matcher"
import { pdfTextRangeToPageRect, type PageHighlightRect, type PdfTextItemLike } from "./pdf-highlight-geometry"

export interface EquationCitationLocator extends CitationLocator {
  locator: string | null
  prefix: string
  suffix: string
}

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value))

export function equationNumber(locator: string | null | undefined): string | null {
  if (!locator) return null
  return locator.match(/(?:式|equation|eq\.?)\s*[\(\[]\s*(\d+)\s*[\)\]]/iu)?.[1] ?? null
}

function fullItemRect(item: PdfTextItemLike, pageWidth: number, pageHeight: number): PageHighlightRect | null {
  return pdfTextRangeToPageRect(item, pageWidth, pageHeight, 0, item.str.length)
}

function rangeRects(citation: EquationCitationLocator, quote: string, items: PdfTextItemLike[], pageWidth: number, pageHeight: number, currentRevision: string | null): PageHighlightRect[] {
  if (!quote.trim()) return []
  const match = matchCitationText({ quote, revision: citation.revision }, items.map(item => item.str), currentRevision)
  return match.ranges.flatMap(range => {
    const item = items[range.spanIndex]
    if (!item) return []
    const rect = pdfTextRangeToPageRect(item, pageWidth, pageHeight, range.start, range.end)
    return rect ? [rect] : []
  })
}

function compact(value: string): string {
  return value.normalize("NFKC").replace(/\s+/gu, "")
}

function isEquationLabel(value: string, number: string): boolean {
  const normalized = compact(value)
  return normalized === `(${number})` || normalized === `[${number}]`
}

function columnBounds(label: PageHighlightRect, anchors: PageHighlightRect[]): { left: number; right: number } {
  if (anchors.length) {
    const left = Math.min(...anchors.map(rect => rect.left))
    const right = Math.max(...anchors.map(rect => rect.left + rect.width))
    if (left < 0.35 && right > 0.65) return { left: 0, right: 1 }
    const center = anchors.reduce((sum, rect) => sum + rect.left + rect.width / 2, 0) / anchors.length
    return center < 0.5 ? { left: 0, right: 0.52 } : { left: 0.48, right: 1 }
  }
  return label.left + label.width / 2 < 0.58 ? { left: 0, right: 0.52 } : { left: 0.48, right: 1 }
}

function unionRects(rects: PageHighlightRect[], paddingX = 0.006, paddingY = 0.006): PageHighlightRect | null {
  if (!rects.length) return null
  const left = Math.min(...rects.map(rect => rect.left))
  const top = Math.min(...rects.map(rect => rect.top))
  const right = Math.max(...rects.map(rect => rect.left + rect.width))
  const bottom = Math.max(...rects.map(rect => rect.top + rect.height))
  return {
    left: clamp(left - paddingX, 0, 1),
    top: clamp(top - paddingY, 0, 1),
    width: clamp(right - left + paddingX * 2, 0, 1),
    height: clamp(bottom - top + paddingY * 2, 0, 1),
  }
}

export function formulaOutlineRegion(rects: PageHighlightRect[]): PageHighlightRect | null {
  return unionRects(rects)
}

export function locateEquationRegion(citation: EquationCitationLocator, items: PdfTextItemLike[], pageWidth: number, pageHeight: number, currentRevision: string | null): PageHighlightRect | null {
  if (currentRevision && citation.revision !== currentRevision) return null
  const number = equationNumber(citation.locator)
  if (!number || pageWidth <= 0 || pageHeight <= 0) return null

  const itemRects = items.map(item => fullItemRect(item, pageWidth, pageHeight))
  const labels = items.flatMap((item, index) => isEquationLabel(item.str, number) && itemRects[index]
    ? [{ index, rect: itemRects[index] as PageHighlightRect }]
    : [])
  if (labels.length !== 1) return null

  const label = labels[0]
  const prefixRects = rangeRects(citation, citation.prefix, items, pageWidth, pageHeight, currentRevision)
  const suffixRects = rangeRects(citation, citation.suffix, items, pageWidth, pageHeight, currentRevision)
  const column = columnBounds(label.rect, [...prefixRects, ...suffixRects])
  const labelCenterY = label.rect.top + label.rect.height / 2
  const typicalHeight = itemRects
    .flatMap(rect => rect ? [rect.height] : [])
    .sort((left, right) => left - right)
    .at(Math.floor(itemRects.length / 2)) ?? label.rect.height
  let top = labelCenterY - Math.max(typicalHeight * 3, 0.025)
  let bottom = labelCenterY + Math.max(typicalHeight * 3, 0.025)

  const prefixBottom = prefixRects.length ? Math.max(...prefixRects.map(rect => rect.top + rect.height)) : null
  const suffixTop = suffixRects.length ? Math.min(...suffixRects.map(rect => rect.top)) : null
  if (prefixBottom !== null && prefixBottom < labelCenterY) top = Math.max(top, prefixBottom + 0.002)
  if (suffixTop !== null && suffixTop > labelCenterY) bottom = Math.min(bottom, suffixTop - 0.002)
  if (bottom <= top) return null

  const formulaItems = itemRects.flatMap((rect, index) => {
    if (!rect) return []
    const centerX = rect.left + rect.width / 2
    const centerY = rect.top + rect.height / 2
    if (centerX < column.left || centerX > column.right || centerY < top || centerY > bottom) return []
    return [{ index, rect }]
  })
  if (!formulaItems.some(value => value.index === label.index)) return null
  const region = unionRects(formulaItems.map(value => value.rect))
  if (!region || region.width < 0.02 || region.width > 0.54 || region.height > 0.16) return null
  if (region.left < column.left - 0.01 || region.left + region.width > column.right + 0.01) return null
  return region
}

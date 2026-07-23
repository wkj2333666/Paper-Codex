export type CitationMatchStatus = "exact" | "fuzzy" | "page-only" | "stale"

export interface CitationLocator {
  quote: string
  revision: string
}

export interface CitationTextRange {
  spanIndex: number
  start: number
  end: number
}

export interface CitationTextMatch {
  status: CitationMatchStatus
  spanIndexes: number[]
  ranges: CitationTextRange[]
}

type SourceOffset = { spanIndex: number; offset: number }
type IndexedText = { text: string; indexes: number[]; offsets: SourceOffset[] }

function indexedText(spans: string[], punctuationInsensitive: boolean): IndexedText {
  const characters: string[] = []
  const indexes: number[] = []
  const offsets: SourceOffset[] = []
  let pendingSpace = false

  spans.forEach((span, spanIndex) => {
    let sourceOffset = 0
    for (const rawCharacter of span) {
      for (const sourceCharacter of rawCharacter.normalize("NFKC").toLocaleLowerCase()) {
        const isWord = /[\p{L}\p{N}]/u.test(sourceCharacter)
        if (punctuationInsensitive && !isWord) continue
        if (!punctuationInsensitive && /\s/u.test(sourceCharacter)) {
          pendingSpace = characters.length > 0
          continue
        }
        if (pendingSpace) {
          characters.push(" ")
          indexes.push(spanIndex)
          offsets.push({ spanIndex, offset: sourceOffset })
          pendingSpace = false
        }
        characters.push(sourceCharacter)
        indexes.push(spanIndex)
        offsets.push({ spanIndex, offset: sourceOffset })
      }
      sourceOffset += rawCharacter.length
    }
    if (!punctuationInsensitive) pendingSpace = characters.length > 0
  })

  const fullText = characters.join("")
  const text = fullText.trim()
  const trimStart = fullText.length - fullText.trimStart().length
  return { text, indexes: indexes.slice(trimStart, trimStart + text.length), offsets: offsets.slice(trimStart, trimStart + text.length) }
}

function normalizedQuote(quote: string, punctuationInsensitive: boolean): string {
  const normalized = quote.normalize("NFKC").toLocaleLowerCase()
  if (punctuationInsensitive) return Array.from(normalized).filter(character => /[\p{L}\p{N}]/u.test(character)).join("")
  return normalized.replace(/\s+/gu, " ").trim()
}

function uniqueRange(document: IndexedText, quote: string): { spanIndexes: number[]; ranges: CitationTextRange[] } | null {
  if (!quote) return null
  const start = document.text.indexOf(quote)
  if (start < 0 || document.text.indexOf(quote, start + 1) >= 0) return null
  const selected = document.indexes.slice(start, start + quote.length)
  const selectedOffsets = document.offsets.slice(start, start + quote.length)
  const ranges = new Map<number, { start: number; end: number }>()
  selectedOffsets.forEach(({ spanIndex, offset }) => {
    const current = ranges.get(spanIndex)
    ranges.set(spanIndex, { start: Math.min(current?.start ?? offset, offset), end: Math.max(current?.end ?? offset + 1, offset + 1) })
  })
  return { spanIndexes: [...new Set(selected)], ranges: [...ranges.entries()].map(([spanIndex, range]) => ({ spanIndex, ...range })) }
}

export function matchCitationText(citation: CitationLocator, spans: string[], currentRevision: string | null): CitationTextMatch {
  if (currentRevision && citation.revision !== currentRevision) return { status: "stale", spanIndexes: [], ranges: [] }

  const exact = uniqueRange(indexedText(spans, false), normalizedQuote(citation.quote, false))
  if (exact) return { status: "exact", ...exact }

  const fuzzyQuote = normalizedQuote(citation.quote, true)
  if (fuzzyQuote.length >= 12) {
    const fuzzy = uniqueRange(indexedText(spans, true), fuzzyQuote)
    if (fuzzy) return { status: "fuzzy", ...fuzzy }
  }

  return { status: "page-only", spanIndexes: [], ranges: [] }
}

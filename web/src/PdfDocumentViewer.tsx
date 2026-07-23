import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { getDocument, GlobalWorkerOptions, TextLayer, type PDFDocumentProxy } from "pdfjs-dist"
import { AnnotationGutter } from "./AnnotationGutter"
import { api } from "./api"
import { matchCitationText, type CitationMatchStatus } from "./citation-matcher"
import { mergeHighlightRects, pdfTextRangeToPageRect, textLayerScaleStyle, textLayerViewportScale } from "./pdf-highlight-geometry"
import { visiblePageWindow } from "./pdf-window"
import type { ResolvedTheme } from "./theme"
import type { MessageCitation, PaperAnnotation } from "./types"

GlobalWorkerOptions.workerSrc = new URL("pdfjs-dist/build/pdf.worker.min.mjs", import.meta.url).toString()

type PageSize = { width: number; height: number }
type HighlightRect = { left: number; top: number; width: number; height: number }
type VisualMatch = { status: CitationMatchStatus; rects: HighlightRect[]; anchorRatio: number }

export function PdfDocumentViewer({ paperId, className = "", citations = [], annotations = [], focusedCitationId = null, currentRevision, onPin, onHide, theme = "light" }: {
  paperId: string
  theme?: ResolvedTheme
  className?: string
  citations?: MessageCitation[]
  annotations?: PaperAnnotation[]
  focusedCitationId?: string | null
  currentRevision?: string | null
  onPin?: (citation: MessageCitation) => void
  onHide?: (citation: MessageCitation) => void
}) {
  const [pdfDocument, setPdfDocument] = useState<PDFDocumentProxy | null>(null)
  const [sizes, setSizes] = useState<PageSize[]>([])
  const [visible, setVisible] = useState({ first: 1, last: 1 })
  const [matches, setMatches] = useState<Record<string, VisualMatch>>({})
  const [error, setError] = useState("")
  const scroller = useRef<HTMLDivElement>(null)
  const savedAnchorSignatures = useRef<Record<string, string>>({})
  const annotationByCitation = useMemo(() => new Map(annotations.filter(annotation => annotation.annotation.state === "visible").map(annotation => [annotation.citation.id, annotation])), [annotations])
  const citationsByPage = useMemo(() => {
    const pages = new Map<number, MessageCitation[]>()
    citations.forEach(citation => pages.set(citation.page, [...(pages.get(citation.page) ?? []), citation]))
    return pages
  }, [citations])

  useEffect(() => {
    let active = true
    setPdfDocument(null); setSizes([]); setMatches({}); setError("")
    const task = getDocument({ url: api.pdfUrl(paperId), httpHeaders: api.authHeaders(), rangeChunkSize: 65_536 })
    void task.promise.then(async value => {
      if (!active) return
      setPdfDocument(value)
      const next = await Promise.all(Array.from({ length: value.numPages }, async (_, index) => {
        const page = await value.getPage(index + 1)
        const viewport = page.getViewport({ scale: 1 })
        return { width: viewport.width, height: viewport.height }
      }))
      if (active) setSizes(next)
    }).catch(value => active && setError(value instanceof Error ? value.message : "PDF 加载失败"))
    return () => { active = false; void task.destroy() }
  }, [paperId])

  useEffect(() => {
    const focused = citations.find(citation => citation.id === focusedCitationId)
    if (!focused || !sizes.length || focused.page > sizes.length) return
    setVisible({ first: focused.page, last: focused.page })
    const frame = requestAnimationFrame(() => scroller.current?.querySelector<HTMLElement>(`[data-pdf-page="${focused.page}"]`)?.scrollIntoView({ behavior: "smooth", block: "center" }))
    return () => cancelAnimationFrame(frame)
  }, [citations, focusedCitationId, sizes.length])

  const rendered = useMemo(() => new Set(visiblePageWindow({ pageCount: sizes.length, firstVisible: visible.first, lastVisible: visible.last, overscan: 2 })), [sizes.length, visible])
  const acceptMatch = useCallback((citationId: string, value: VisualMatch) => {
    const annotation = annotationByCitation.get(citationId)
    const citation = citations.find(item => item.id === citationId)
    setMatches(current => ({ ...current, [citationId]: value }))
    if (!citation || !annotation || !value.rects.length || !["exact", "fuzzy"].includes(value.status)) return
    const signature = JSON.stringify(value.rects)
    if (signature === savedAnchorSignatures.current[citationId]) return
    savedAnchorSignatures.current[citationId] = signature
    void api.replaceAnnotationAnchors(annotation.annotation.id, value.rects.map((rect, rectIndex) => ({ page: citation.page, rect_index: rectIndex, x: rect.left, y: rect.top, width: rect.width, height: rect.height }))).catch(() => {})
  }, [annotationByCitation, citations])

  const updateVisible = () => {
    const root = scroller.current
    if (!root) return
    const rootRect = root.getBoundingClientRect()
    const hits = Array.from(root.querySelectorAll<HTMLElement>("[data-pdf-page]"))
      .filter(page => { const rect = page.getBoundingClientRect(); return rect.bottom > rootRect.top && rect.top < rootRect.bottom })
      .map(page => Number(page.dataset.pdfPage))
    if (hits.length) setVisible({ first: Math.min(...hits), last: Math.max(...hits) })
  }

  if (error) return <div className="pdf-error">无法显示 PDF：{error}</div>
  return <div className={`pdf-viewer ${theme === "dark" ? "dark-reader " : ""}${className}`} ref={scroller} onScroll={updateVisible}>
    {!pdfDocument || !sizes.length
      ? <div className="pdf-loading">正在加载论文原文…</div>
      : sizes.map((size, index) => {
          const pageNumber = index + 1
          const pageCitations = citationsByPage.get(pageNumber) ?? []
          const pageMatches = pageCitations.map(citation => ({ citation, match: matches[citation.id] ?? { status: annotationByCitation.get(citation.id)?.annotation.availability === "revision-stale" ? "stale" as const : "page-only" as const, rects: [], anchorRatio: 0.08 } }))
          const pageItems = pageCitations.map(citation => {
            const annotation = annotationByCitation.get(citation.id)
            return { citation, body: annotation?.annotation.body, status: matches[citation.id]?.status ?? (annotation?.annotation.availability === "revision-stale" ? "stale" : "page-only"), anchorRatio: matches[citation.id]?.anchorRatio ?? 0.08, pinned: Boolean(annotation), focused: citation.id === focusedCitationId, onPin: () => onPin?.(citation), onUnpin: () => onHide?.(citation) }
          })
          return <div className={`pdf-page-row${pageCitations.length ? " has-annotation" : ""}`} key={pageNumber}>
            <div className="pdf-page-shell" data-pdf-page={pageNumber} style={{ aspectRatio: `${size.width}/${size.height}`, width: `${Math.round(size.width * 1.35)}px`, maxWidth: "100%" }}>
              {rendered.has(pageNumber)
                ? <PdfPage document={pdfDocument} pageNumber={pageNumber} citations={pageCitations} matches={pageMatches} focusedCitationId={focusedCitationId} currentRevision={currentRevision ?? null} onMatch={acceptMatch}/>
                : <span className="pdf-page-number">{pageNumber}</span>}
            </div>
            {pageItems.length > 0 && <AnnotationGutter items={pageItems} focusedCitationId={focusedCitationId}/>}
          </div>
        })}
  </div>
}

function PdfPage({ document: pdfDocument, pageNumber, citations, matches, focusedCitationId, currentRevision, onMatch }: {
  document: PDFDocumentProxy
  pageNumber: number
  citations: MessageCitation[]
  matches: Array<{ citation: MessageCitation; match: VisualMatch }>
  focusedCitationId: string | null
  currentRevision: string | null
  onMatch: (citationId: string, match: VisualMatch) => void
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const textRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    let active = true
    let renderTask: { cancel: () => void } | null = null
    let textLayer: TextLayer | null = null
    void pdfDocument.getPage(pageNumber).then(async page => {
      if (!active) return
      const viewport = page.getViewport({ scale: 1.35 })
      const canvas = canvasRef.current
      const text = textRef.current
      if (!canvas || !text) return
      const ratio = Math.min(window.devicePixelRatio || 1, 2)
      canvas.width = Math.floor(viewport.width * ratio); canvas.height = Math.floor(viewport.height * ratio)
      canvas.style.width = `${viewport.width}px`; canvas.style.height = `${viewport.height}px`
      const context = canvas.getContext("2d")
      if (!context) return
      renderTask = page.render({ canvas, canvasContext: context, viewport, transform: ratio === 1 ? undefined : [ratio, 0, 0, ratio, 0, 0] })
      await (renderTask as ReturnType<typeof page.render>).promise
      if (!active) return
      text.replaceChildren()
      text.style.setProperty("--total-scale-factor", textLayerScaleStyle(viewport.scale))
      const textViewport = page.getViewport({ scale: textLayerViewportScale(viewport.scale, window.devicePixelRatio || 1) })
      const textContent = await page.getTextContent()
      textLayer = new TextLayer({ textContentSource: textContent, container: text, viewport: textViewport })
      await textLayer.render()
      if (!active) return
      const spans = Array.from(textLayer.textDivs)
      const textItems = textContent.items
        .filter(item => "str" in item && typeof item.str === "string" && item.str.length > 0)
        .map(item => item as { str: string; transform: number[]; width: number; height: number })
      const pageWidth = viewport.width / viewport.scale
      const pageHeight = viewport.height / viewport.scale
      const measureCanvas = globalThis.document.createElement("canvas").getContext("2d")
      citations.forEach(citation => {
        const located = matchCitationText(citation, textItems.map(item => item.str), currentRevision)
        const rects = mergeHighlightRects(located.ranges.flatMap(range => {
          const item = textItems[range.spanIndex]
          if (!item) return []
          const span = spans[range.spanIndex]
          const style = span ? globalThis.getComputedStyle(span) : null
          const measure = style && measureCanvas ? (value: string) => {
            measureCanvas.font = style.font || `${style.fontSize} ${style.fontFamily}`
            return measureCanvas.measureText(value).width
          } : undefined
          const rect = pdfTextRangeToPageRect(item, pageWidth, pageHeight, range.start, range.end, measure)
          return rect ? [rect] : []
        }))
        onMatch(citation.id, { status: located.status, rects, anchorRatio: rects[0]?.top ?? 0.08 })
      })
    }).catch(() => {})
    return () => { active = false; renderTask?.cancel(); textLayer?.cancel() }
  }, [citations, currentRevision, onMatch, pageNumber, pdfDocument])

  return <div className="pdf-page"><canvas ref={canvasRef}/><div className="textLayer" ref={textRef}/><CitationHighlights matches={matches} focusedCitationId={focusedCitationId}/><span className="pdf-page-number">{pageNumber}</span></div>
}

function CitationHighlights({ matches, focusedCitationId }: { matches: Array<{ citation: MessageCitation; match: VisualMatch }>; focusedCitationId: string | null }) {
  return <>{matches.flatMap(({ citation, match }) => match.rects.length
    ? match.rects.map((rect, index) => <span className={`citation-highlight${citation.id === focusedCitationId ? " focused" : ""}`} data-citation-id={citation.id} key={`${citation.id}:${index}`} style={{ left: `${rect.left * 100}%`, top: `${rect.top * 100}%`, width: `${rect.width * 100}%`, height: `${rect.height * 100}%` }}/>)
    : <span className="citation-location-note" data-citation-id={citation.id} key={`${citation.id}:note`} style={{ top: `${Math.min(88, 8 + matches.findIndex(item => item.citation.id === citation.id) * 8)}%` }}>{match.status === "stale" ? `第 ${citation.page} 页 · 版本已变化` : `第 ${citation.page} 页 · 未找到精确文本`}</span>)} </>
}

import { Maximize2, Minimize2, ZoomIn, ZoomOut } from "lucide-react"

export const PDF_ZOOM_MIN = 75
export const PDF_ZOOM_MAX = 200
export const PDF_ZOOM_STEP = 25

export function clampPdfZoom(zoom: number): number {
  if (!Number.isFinite(zoom)) return 100
  return Math.min(PDF_ZOOM_MAX, Math.max(PDF_ZOOM_MIN, Math.round(zoom)))
}

export async function togglePdfFullscreen(element: HTMLElement, fullscreenDocument: Document = document) {
  if (fullscreenDocument.fullscreenElement === element) {
    await fullscreenDocument.exitFullscreen()
    return
  }
  await element.requestFullscreen()
}

export function PdfFullscreenButton({ fullscreen, onToggle }: {
  fullscreen: boolean
  onToggle: () => void | Promise<void>
}) {
  const label = fullscreen ? "退出全屏阅读" : "全屏阅读论文"
  const Icon = fullscreen ? Minimize2 : Maximize2
  return <button
    type="button"
    className="pdf-fullscreen-button"
    aria-label={label}
    aria-pressed={fullscreen}
    title={label}
    onClick={() => void onToggle()}
  >
    <Icon />
    <span>{fullscreen ? "退出全屏" : "全屏阅读"}</span>
  </button>
}

export function PdfReaderToolbar({ fullscreen, zoom, onToggle, onZoomChange }: {
  fullscreen: boolean
  zoom: number
  onToggle: () => void | Promise<void>
  onZoomChange: (zoom: number) => void
}) {
  const updateZoom = (value: number) => onZoomChange(clampPdfZoom(value))
  return <div className="pdf-reader-toolbar-content">
    {fullscreen && <div className="pdf-zoom-controls" role="group" aria-label="PDF 缩放">
      <button type="button" aria-label="缩小 PDF" title="缩小" disabled={zoom <= PDF_ZOOM_MIN} onClick={() => updateZoom(zoom - PDF_ZOOM_STEP)}><ZoomOut /></button>
      <input aria-label="PDF 缩放" type="range" min={PDF_ZOOM_MIN} max={PDF_ZOOM_MAX} step={5} value={zoom} onChange={event => updateZoom(Number(event.target.value))}/>
      <button type="button" aria-label="放大 PDF" title="放大" disabled={zoom >= PDF_ZOOM_MAX} onClick={() => updateZoom(zoom + PDF_ZOOM_STEP)}><ZoomIn /></button>
      <button type="button" className="pdf-zoom-value" aria-label="恢复 PDF 到 100%" title="恢复 100%" onClick={() => updateZoom(100)}>{zoom}%</button>
    </div>}
    <PdfFullscreenButton fullscreen={fullscreen} onToggle={onToggle}/>
  </div>
}

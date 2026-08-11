import { Maximize2, Minimize2 } from "lucide-react"

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

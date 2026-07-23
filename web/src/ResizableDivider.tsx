import { useRef } from "react"
import type { KeyboardEvent, PointerEvent } from "react"
import type { PanelName } from "./panel-preferences"

export interface ResizableDividerProps {
  panel: PanelName
  value: number
  min: number
  max: number
  onResize: (delta: number) => void
  onReset: () => void
}

export function dividerDeltaForKey(key: string, coarse: boolean): number | null {
  const step = coarse ? 40 : 10
  if (key === "ArrowLeft") return -step
  if (key === "ArrowRight") return step
  return null
}

export function ResizableDivider({ panel, value, min, max, onResize, onReset }: ResizableDividerProps) {
  const lastX = useRef<number | null>(null)

  const pointerDown = (event: PointerEvent<HTMLDivElement>) => {
    lastX.current = event.clientX
    event.currentTarget.setPointerCapture(event.pointerId)
  }
  const pointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (lastX.current === null || !event.currentTarget.hasPointerCapture(event.pointerId)) return
    const delta = event.clientX - lastX.current
    lastX.current = event.clientX
    if (delta) onResize(delta)
  }
  const pointerEnd = () => { lastX.current = null }
  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const delta = dividerDeltaForKey(event.key, event.shiftKey)
    if (delta === null) return
    event.preventDefault()
    onResize(delta)
  }

  return <div
    className={`resizable-divider resizable-divider-${panel}`}
    role="separator"
    aria-label={`调整${panel === "sidebar" ? "文件树" : panel === "paperGraph" ? "知识图谱" : "Codex"}宽度`}
    aria-orientation="vertical"
    aria-valuemin={min}
    aria-valuemax={max}
    aria-valuenow={value}
    tabIndex={0}
    onPointerDown={pointerDown}
    onPointerMove={pointerMove}
    onPointerUp={pointerEnd}
    onPointerCancel={pointerEnd}
    onLostPointerCapture={pointerEnd}
    onKeyDown={keyDown}
    onDoubleClick={onReset}
  />
}

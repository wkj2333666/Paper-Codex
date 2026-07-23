import { useEffect, useRef, useState } from "react"
import { ChevronDown, ChevronUp, Eye, EyeOff, GripVertical, Pin, PinOff } from "lucide-react"
import { placeAnnotationCards } from "./annotation-layout"
import type { CitationMatchStatus } from "./citation-matcher"
import type { MessageCitation } from "./types"

export interface CardPreference { collapsed: boolean; hidden: boolean; offset: number }
export const defaultCardPreference: CardPreference = { collapsed: true, hidden: false, offset: 0 }

export interface AnnotationGutterItem {
  citation: MessageCitation
  body?: string
  status: CitationMatchStatus
  anchorRatio?: number
  pinned?: boolean
  focused?: boolean
  onPin?: () => void
  onUnpin?: () => void
}

export function isCompactAnnotationGutter(items: AnnotationGutterItem[], preferences: Record<string, CardPreference>): boolean {
  return items.every(item => {
    const preference = preferences[item.citation.id] ?? defaultCardPreference
    return preference.hidden || preference.collapsed
  })
}

function preferenceKey(id: string) { return `paper-codex.annotation-card.${id}` }

function loadPreference(id: string): CardPreference {
  try { return { ...defaultCardPreference, ...JSON.parse(localStorage.getItem(preferenceKey(id)) ?? "{}") } }
  catch { return defaultCardPreference }
}

const statusLabel: Record<CitationMatchStatus, string> = {
  exact: "已定位原文",
  fuzzy: "近似定位",
  "page-only": "已定位页码",
  stale: "论文版本已变化",
}

export function AnnotationGutter({ items, focusedCitationId, onFocus }: {
  items: AnnotationGutterItem[]
  focusedCitationId?: string | null
  onFocus?: (citation: MessageCitation) => void
}) {
  const [preferences, setPreferences] = useState<Record<string, CardPreference>>({})
  const [placements, setPlacements] = useState<Record<string, number>>({})
  const gutterRef = useRef<HTMLDivElement>(null)
  const cardRefs = useRef<Record<string, HTMLDivElement | null>>({})
  const itemKey = items.map(item => `${item.citation.id}:${item.anchorRatio ?? 0.08}:${item.status}`).join("|")

  useEffect(() => {
    setPreferences(Object.fromEntries(items.map(item => [item.citation.id, loadPreference(item.citation.id)])))
  }, [itemKey])

  useEffect(() => {
    Object.entries(preferences).forEach(([id, preference]) => {
      try { localStorage.setItem(preferenceKey(id), JSON.stringify(preference)) } catch { /* private browsing */ }
    })
  }, [preferences])

  useEffect(() => {
    if (!focusedCitationId) return
    setPreferences(value => ({ ...value, [focusedCitationId]: { ...(value[focusedCitationId] ?? defaultCardPreference), collapsed: false } }))
  }, [focusedCitationId])

  useEffect(() => {
    const gutter = gutterRef.current
    if (!gutter) return
    const update = () => {
      const requests = items
        .filter(item => !(preferences[item.citation.id] ?? defaultCardPreference).hidden)
        .map(item => {
          const preference = preferences[item.citation.id] ?? defaultCardPreference
          return {
            id: item.citation.id,
            preferredTop: gutter.clientHeight * (item.anchorRatio ?? 0.08) + preference.offset,
            height: cardRefs.current[item.citation.id]?.offsetHeight ?? 44,
          }
        })
      setPlacements(Object.fromEntries(placeAnnotationCards(requests, gutter.clientHeight, 8).map(item => [item.id, item.top])))
    }
    update()
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(update)
    observer.observe(gutter)
    Object.values(cardRefs.current).forEach(card => card && observer.observe(card))
    return () => observer.disconnect()
  }, [itemKey, preferences])

  const updatePreference = (id: string, patch: Partial<CardPreference>) => {
    setPreferences(value => ({ ...value, [id]: { ...(value[id] ?? defaultCardPreference), ...patch } }))
  }

  const compact = isCompactAnnotationGutter(items, preferences)

  return <aside className={`annotation-gutter${compact ? " compact" : ""}`} data-layout={compact ? "compact" : "expanded"} ref={gutterRef} aria-label="Codex 原文说明">
    {items.map(item => {
      const id = item.citation.id
      const preference = preferences[id] ?? defaultCardPreference
      const top = placements[id] ?? 12
      if (preference.hidden) return <button className="annotation-restore" style={{ top }} key={id} onClick={() => updatePreference(id, { hidden: false })}><Eye/>显示 Codex 说明</button>
      const startDrag = (event: React.PointerEvent) => {
        if ((event.target as HTMLElement).closest("button")) return
        event.preventDefault()
        const startY = event.clientY
        const startOffset = preference.offset
        const move = (moveEvent: PointerEvent) => updatePreference(id, { offset: startOffset + moveEvent.clientY - startY })
        const finish = () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", finish) }
        window.addEventListener("pointermove", move); window.addEventListener("pointerup", finish)
      }
      const toggle = (event: React.MouseEvent) => {
        if ((event.target as HTMLElement).closest("button")) return
        onFocus?.(item.citation)
        updatePreference(id, { collapsed: !preference.collapsed })
      }
      return <div className={`annotation-card${preference.collapsed ? " collapsed" : ""}${item.focused ? " focused" : ""}`} ref={card => { cardRefs.current[id] = card }} style={{ top }} key={id}>
        <header onPointerDown={startDrag} onClick={toggle}><GripVertical/><span>第 {item.citation.page} 页 · Codex 说明</span><em>{statusLabel[item.status]}</em>
          <button className="annotation-pin" aria-label={item.pinned ? "取消固定" : "固定说明"} title={item.pinned ? "取消固定" : "固定说明"} onPointerDown={event => event.stopPropagation()} onClick={item.pinned ? item.onUnpin : item.onPin}>{item.pinned ? <PinOff/> : <Pin/>}</button>
          <button className="annotation-toggle" aria-label={preference.collapsed ? "展开说明" : "缩小说明"} onPointerDown={event => event.stopPropagation()} onClick={() => updatePreference(id, { collapsed: !preference.collapsed })}>{preference.collapsed ? <ChevronDown/> : <ChevronUp/>}</button>
          <button className="annotation-hide" aria-label="隐藏说明" onPointerDown={event => event.stopPropagation()} onClick={() => updatePreference(id, { hidden: true })}><EyeOff/></button>
        </header>
        {!preference.collapsed && <div className="annotation-body"><p>{item.body || item.citation.explanation || "这段原文支持 Codex 回答中的对应结论。"}</p><blockquote>{item.citation.quote}</blockquote>{item.citation.section && <span>{item.citation.section}{item.citation.locator ? ` · ${item.citation.locator}` : ""}</span>}</div>}
      </div>
    })}
  </aside>
}

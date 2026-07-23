import { Bot, ChevronLeft, ChevronRight, FolderTree, Network } from "lucide-react"
import type { PanelName } from "./panel-preferences"

const icons = {
  sidebar: FolderTree,
  paperGraph: Network,
  codex: Bot,
}

export function PanelRail({ panel, label, side, onExpand, className = "" }: {
  panel: PanelName
  label: string
  side: "left" | "right"
  onExpand: (trigger: HTMLButtonElement) => void
  className?: string
}) {
  const Icon = icons[panel]
  return <div className={`panel-rail panel-rail-${side} ${className}`.trim()} data-panel={panel}>
    <button
      type="button"
      aria-label={`展开${label}`}
      aria-expanded={false}
      onClick={event => onExpand(event.currentTarget)}
    >
      <Icon />
      <span>{label}</span>
    </button>
  </div>
}

export function PanelCollapseButton({ label, direction, onCollapse }: {
  label: string
  direction: "left" | "right"
  onCollapse: () => void
}) {
  const Icon = direction === "left" ? ChevronLeft : ChevronRight
  return <button
    type="button"
    className="panel-collapse"
    aria-label={`收起${label}`}
    aria-expanded={true}
    title={`收起${label}`}
    onClick={onCollapse}
  >
    <Icon />
  </button>
}

export function MobilePanelRails({ showPaperGraph, showCodex = true, onOpen }: {
  showPaperGraph: boolean
  showCodex?: boolean
  onOpen: (panel: PanelName, trigger: HTMLButtonElement) => void
}) {
  return <div className="mobile-panel-rails">
    <PanelRail panel="sidebar" label="文件树" side="left" onExpand={trigger => onOpen("sidebar", trigger)} />
    <div className="mobile-panel-rails-right">
      {showPaperGraph && <PanelRail panel="paperGraph" label="相关知识" side="right" onExpand={trigger => onOpen("paperGraph", trigger)} />}
      {showCodex && <PanelRail panel="codex" label="Codex" side="right" onExpand={trigger => onOpen("codex", trigger)} />}
    </div>
  </div>
}

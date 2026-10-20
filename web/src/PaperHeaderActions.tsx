import { FileText, Network, Trash2 } from "lucide-react"
import type { ReactNode } from "react"
import "./paper-actions.css"

export function PaperHeaderActions({
  analysisAction,
  paperGraphOpen,
  sourceUrl,
  onTrash,
  onToggleGraph,
}: {
  analysisAction: ReactNode
  paperGraphOpen: boolean
  sourceUrl: string | null
  onTrash: () => void
  onToggleGraph: (trigger: HTMLButtonElement) => void
}) {
  return <div className="paper-head-actions">
    {sourceUrl ? <a className="outline paper-source-link" href={sourceUrl} target="_blank" rel="noopener noreferrer"><FileText/>打开原始链接</a> : <button className="outline" disabled title="本地上传的论文没有原始链接"><FileText/>无原始链接</button>}
    {analysisAction}
    <button className="danger-outline" onClick={onTrash}><Trash2/>移入回收站</button>
    <button
      className="outline paper-graph-action"
      aria-controls="paper-graph-panel"
      aria-expanded={paperGraphOpen}
      onClick={event=>onToggleGraph(event.currentTarget)}
    ><Network/>相关知识</button>
  </div>
}

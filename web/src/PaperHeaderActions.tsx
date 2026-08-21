import { FileText, LoaderCircle, Network, Sparkles, Trash2 } from "lucide-react"

export function PaperHeaderActions({
  reanalyzing,
  paperGraphOpen,
  onOpenPdf,
  onReanalyze,
  onTrash,
  onToggleGraph,
}: {
  reanalyzing: boolean
  paperGraphOpen: boolean
  onOpenPdf: () => void
  onReanalyze: () => void
  onTrash: () => void
  onToggleGraph: (trigger: HTMLButtonElement) => void
}) {
  return <div className="paper-head-actions">
    <button className="outline" onClick={onOpenPdf}><FileText/>阅读原文</button>
    <button className="outline" disabled={reanalyzing} onClick={onReanalyze}>{reanalyzing?<LoaderCircle className="spin"/>:<Sparkles/>}重新分析</button>
    <button className="danger-outline" onClick={onTrash}><Trash2/>移入回收站</button>
    <button
      className="outline paper-graph-action"
      aria-controls="paper-graph-panel"
      aria-expanded={paperGraphOpen}
      onClick={event=>onToggleGraph(event.currentTarget)}
    ><Network/>相关知识</button>
  </div>
}

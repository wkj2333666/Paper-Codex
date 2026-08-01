import { lazy, Suspense } from "react"
import { BookOpen, Compass, Flag, Network, Sparkles } from "lucide-react"
import { buildProjectOverview } from "./project-overview"
import type { ResolvedTheme } from "./theme"
import type { GraphPayload, LiteratureSearchRun, Paper, Project, ProjectCandidate, ProjectGoalSummary } from "./types"

const SemanticGraph=lazy(()=>import("./SemanticGraph").then(module=>({default:module.SemanticGraph})))

export function ProjectOverview({project,papers,candidates,searches,goals,graph,theme,onOpenPaper}:{project:Project;papers:Paper[];candidates:ProjectCandidate[];searches:LiteratureSearchRun[];goals:ProjectGoalSummary[];graph:GraphPayload;theme:ResolvedTheme;onOpenPaper:(id:string)=>void}){
  const overview=buildProjectOverview({project,papers,candidates,searches,goals,graph})
  return <div className="project-overview">
    <div className="project-overview-metrics">
      <div><BookOpen/><strong>{overview.metrics.papers}</strong><span>正式论文</span></div>
      <div><Sparkles/><strong>{overview.metrics.candidates}</strong><span>活跃候选</span></div>
      <div><Flag/><strong>{overview.metrics.activeGoals}</strong><span>进行中 Goal</span></div>
      <div><Network/><strong>{overview.metrics.hypotheses}</strong><span>待验证关系</span></div>
    </div>
    <div className="project-overview-grid">
      <section><header><Flag/><div><h3>最重要的工作</h3><p>先处理会阻塞研究推进的事项</p></div></header>{overview.importantWork.length?<ol className="overview-work-list">{overview.importantWork.map(item=><li key={item.id} data-kind={item.kind}><span>{item.kind==="goal"?"Goal":item.kind==="import"?"导入":item.kind==="candidate"?"候选":"检索"}</span><div><strong>{item.title}</strong><p>{item.detail}</p></div></li>)}</ol>:<p className="overview-empty">暂无紧急事项。可在项目对话中设置研究 Goal。</p>}</section>
      <section><header><Compass/><div><h3>方向建议</h3><p>来自图谱假设与未处理候选</p></div></header>{overview.directions.length?<ul className="overview-direction-list">{overview.directions.map(item=><li key={item.id}><strong>{item.title}</strong><p>{item.detail}</p></li>)}</ul>:<p className="overview-empty">积累更多论文后，这里会形成可验证的研究方向。</p>}</section>
      <section className="overview-graph"><header><Network/><div><h3>项目知识图谱</h3><p>论文、方法、概念和发现的连接</p></div></header>{graph.nodes.length?<Suspense fallback={<p className="overview-empty">正在加载知识图谱…</p>}><SemanticGraph compact theme={theme} payload={graph} onPaperOpen={onOpenPaper}/></Suspense>:<p className="overview-empty">导入并完成智能评阅后，图谱会在这里生长。</p>}</section>
      <section><header><BookOpen/><div><h3>关键论文</h3><p>按项目图谱连接数排序</p></div></header>{overview.keyPapers.length?<ul className="overview-paper-list">{overview.keyPapers.map(paper=><li key={paper.id}><button onClick={()=>onOpenPaper(paper.id)}><strong>{paper.title}</strong><span>{paper.year??"年份未知"} · {paper.connections} 个连接</span></button></li>)}</ul>:<p className="overview-empty">项目尚无正式论文。</p>}</section>
    </div>
  </div>
}

import { Component, lazy, Suspense, type ReactNode } from "react"
import { Activity, BookOpen, Compass, Flag, Network, Sparkles } from "lucide-react"
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
      <section><header><BookOpen/><div><h3>最重要的工作</h3><p>项目中关联最紧密的正式论文</p></div></header>{overview.importantWorks.length?<ol className="overview-paper-list">{overview.importantWorks.map(paper=><li key={paper.id}><button onClick={()=>onOpenPaper(paper.id)}><strong>{paper.title}</strong><span>{paper.year??"年份未知"} · {paper.connections} 个图谱关联</span></button></li>)}</ol>:<p className="overview-empty">项目尚无正式论文。导入论文后会按图谱关联形成排序。</p>}</section>
      <section><header><Compass/><div><h3>方向建议</h3><p>来自图谱假设与未处理候选</p></div></header>{overview.directions.length?<ul className="overview-direction-list">{overview.directions.map(item=><li key={item.id}><strong>{item.title}</strong><p>{item.detail}</p></li>)}</ul>:<p className="overview-empty">积累更多论文后，这里会形成可验证的研究方向。</p>}</section>
      <section className="overview-graph"><header><Network/><div><h3>项目知识图谱</h3><p>论文、方法、概念和发现的连接</p></div></header>{graph.nodes.length?<ProjectGraphErrorBoundary><Suspense fallback={<p className="overview-empty">正在加载知识图谱…</p>}><SemanticGraph compact theme={theme} payload={graph} onPaperOpen={onOpenPaper}/></Suspense></ProjectGraphErrorBoundary>:<p className="overview-empty">导入并完成智能评阅后，图谱会在这里生长。</p>}</section>
      <section><header><Activity/><div><h3>研究进展</h3><p>当前 Goal、检索与导入状态</p></div></header>{overview.progress.length?<ol className="overview-progress-list">{overview.progress.map(item=><li key={item.id} data-kind={item.kind}><span>{item.kind==="goal"?"Goal":item.kind==="import"?"导入":"检索"}</span><div><strong>{item.title}</strong><p>{item.detail}</p></div></li>)}</ol>:<p className="overview-empty">当前没有正在进行的 Goal、检索或导入。</p>}</section>
    </div>
  </div>
}

class ProjectGraphErrorBoundary extends Component<{children:ReactNode},{failed:boolean}> {
  state={failed:false}

  static getDerivedStateFromError(){return {failed:true}}

  render(){
    return this.state.failed
      ? <p className="overview-empty">当前浏览器无法显示项目图谱，但项目论文和研究资料仍可正常使用。</p>
      : this.props.children
  }
}

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import {
  BookOpen,
  CheckCircle2,
  Clock3,
  ExternalLink,
  FileSearch,
  FileText,
  History,
  LoaderCircle,
  LayoutDashboard,
  NotebookPen,
  RotateCcw,
  Trash2,
  X,
} from "lucide-react"
import { api } from "./api"
import { ProjectOverview } from "./ProjectOverview"
import type { ResolvedTheme } from "./theme"
import type {
  GraphPayload,
  LiteratureSearchDetail,
  LiteratureSearchRun,
  Paper,
  Project,
  ProjectCandidate,
  ProjectGoalSummary,
} from "./types"

export type ProjectResearchTab="overview"|"notes"|"papers"|"candidates"|"searches"

export function shouldReloadProjectResearch(event:{type:string;payload:Record<string,unknown>},projectId:string){
  return event.type==="project-research-revised"&&event.payload.project_id===projectId
}

export class ProjectResearchReloadCoordinator {
  private generation=0

  invalidate(){this.generation+=1}

  async run<T>(load:()=>Promise<T>,apply:(value:T)=>void,onError?:(error:unknown)=>void){
    const generation=++this.generation
    try{
      const value=await load()
      if(generation!==this.generation)return false
      apply(value)
      return true
    }catch(error){
      if(generation===this.generation)onError?.(error)
      return false
    }
  }
}

export interface CandidateActions {
  dismiss:(workId:string)=>Promise<void>
  restore:(workId:string)=>Promise<void>
  remove:(workId:string)=>Promise<void>
  importCandidate:(workId:string)=>Promise<void>
}

export function createCandidateActions(
  projectId:string,
  refresh:()=>Promise<void>,
  openPaper:(paperId:string)=>void,
):CandidateActions{
  const update=async(workId:string,status:"candidate"|"dismissed")=>{
    await api.updateCandidate(projectId,workId,{status})
    await refresh()
  }
  return {
    dismiss:workId=>update(workId,"dismissed"),
    restore:workId=>update(workId,"candidate"),
    remove:async workId=>{await api.removeCandidate(projectId,workId);await refresh()},
    importCandidate:async workId=>{
      const result=await api.importCandidate(projectId,workId)
      if("paper_id" in result)openPaper(result.paper_id)
      await refresh()
    },
  }
}

export interface ProjectResearchViewProps {
  tab:ProjectResearchTab
  papers:Paper[]
  candidates:ProjectCandidate[]
  searches:LiteratureSearchRun[]
  includeDismissed:boolean
  busy:boolean
  error:string
  actions:CandidateActions
  onTab:(tab:ProjectResearchTab)=>void
  onOpenCandidate:(candidate:ProjectCandidate)=>void
  onOpenPaper:(paperId:string)=>void
  onRemovePaper:(paperId:string)=>void
  onToggleDismissed:()=>void
  onOpenSearch:(search:LiteratureSearchRun)=>void
  overview?:ReactNode
  notes?:ReactNode
}

const evidenceLabel=(value:ProjectCandidate["evidence_level"])=>({
  metadata:"仅元数据",
  abstract:"已核验摘要",
  fulltext:"已核验全文",
})[value]

const candidateStatusLabel=(value:ProjectCandidate["status"])=>({
  candidate:"待确认",
  importing:"正在导入",
  imported:"已加入项目",
  dismissed:"暂不考虑",
})[value]

export function ProjectResearchView({
  tab,papers,candidates,searches,includeDismissed,busy,error,actions,onTab,
  onOpenCandidate,onOpenPaper,onRemovePaper,onToggleDismissed,onOpenSearch,overview,notes,
}:ProjectResearchViewProps){
  const activeSearches=searches.filter(item=>item.state==="running").length
  return <section className="project-research" aria-label="项目研究资料">
    <nav className="project-research-tabs" aria-label="项目资料分类">
      <button className={tab==="overview"?"active":""} onClick={()=>onTab("overview")}><LayoutDashboard/>综合视图</button>
      <button className={tab==="notes"?"active":""} onClick={()=>onTab("notes")}><NotebookPen/>项目笔记</button>
      <button className={tab==="papers"?"active":""} onClick={()=>onTab("papers")}><BookOpen/>项目论文 <em>{papers.length}</em></button>
      <button className={tab==="candidates"?"active":""} onClick={()=>onTab("candidates")}><FileSearch/>候选论文 <em>{candidates.filter(item=>item.status!=="dismissed").length}</em></button>
      <button className={tab==="searches"?"active":""} onClick={()=>onTab("searches")}><History/>检索历史 {activeSearches>0&&<em>{activeSearches}</em>}</button>
    </nav>
    {error&&<p className="project-research-error" role="alert">{error}</p>}
    {busy&&<div className="project-research-loading" role="status"><LoaderCircle className="spin"/>正在加载项目研究资料…</div>}
    {!busy&&tab==="overview"&&overview}
    {!busy&&tab==="notes"&&notes}
    {!busy&&tab==="papers"&&(papers.length?<div className="project-paper-list">{papers.map(paper=><article key={paper.id}>
      <button className="project-paper-main" onClick={()=>onOpenPaper(paper.id)}><FileText/><span><strong>{paper.title}</strong><small>{paper.year??"年份未知"} · {paper.doi??paper.arxiv_id??paper.id}</small></span></button>
      <button className="icon-action" aria-label={`移出项目：${paper.title}`} onClick={()=>onRemovePaper(paper.id)}><X/></button>
    </article>)}</div>:<ResearchEmpty title="这个项目还没有正式论文" text="让 Codex 检索候选，确认后再加入项目并分析。"/>)}
    {!busy&&tab==="candidates"&&<>
      <div className="candidate-toolbar"><p>候选只属于当前项目；确认导入后才会成为正式论文。</p><button onClick={onToggleDismissed}>{includeDismissed?"隐藏暂不考虑":"显示暂不考虑"}</button></div>
      {candidates.length?<div className="candidate-grid">{candidates.map(candidate=><CandidateCard key={candidate.work.id} candidate={candidate} actions={actions} onOpen={()=>onOpenCandidate(candidate)} onOpenPaper={onOpenPaper}/>)}</div>:<ResearchEmpty title="还没有候选论文" text="在右侧 Codex 项目对话中讨论选题，并开启文献检索。"/>}
    </>}
    {!busy&&tab==="searches"&&(searches.length?<div className="research-history">{searches.map(search=><button key={search.id} onClick={()=>onOpenSearch(search)}>
      <div><span className={`search-state ${search.state}`}>{searchStateLabel(search.state)}</span><time>{formatTime(search.created_at)}</time></div>
      <strong>{search.question}</strong>
      <p>{providerSummary(search)}</p>
      <small>{search.trigger==="explicit"?"手动检索":"Codex 自动检索"}</small>
    </button>)}</div>:<ResearchEmpty title="还没有检索记录" text="项目对话中的每次论文检索都会保留在这里。"/>)}
  </section>
}

function CandidateCard({candidate,actions,onOpen,onOpenPaper}:{candidate:ProjectCandidate;actions:CandidateActions;onOpen:()=>void;onOpenPaper:(paperId:string)=>void}){
  const status=candidate.status
  return <article className={`candidate-card status-${status}`}>
    <button className="candidate-card-main" onClick={onOpen}>
      <div className="candidate-card-meta"><span className={`evidence-badge ${candidate.evidence_level}`}>{evidenceLabel(candidate.evidence_level)}</span><span>{candidate.work.year??"年份未知"}</span></div>
      <h3>{candidate.work.title}</h3>
      <p className="candidate-authors">{candidate.work.authors.join("、")||"作者未知"}</p>
      <p className="candidate-reason">{candidate.relevance_reason}</p>
      <span className={`candidate-status ${status}`}>{candidateStatusLabel(status)}</span>
    </button>
    <div className="candidate-actions" aria-live="polite">
      {status==="candidate"&&<>
        <button className="primary-action" onClick={()=>void actions.importCandidate(candidate.work.id)}>加入项目并分析</button>
        <button onClick={()=>void actions.dismiss(candidate.work.id)}>暂不考虑</button>
        <button className="icon-action danger-action" aria-label={`移除候选：${candidate.work.title}`} onClick={()=>void actions.remove(candidate.work.id)}><Trash2/></button>
      </>}
      {status==="dismissed"&&<>
        <button onClick={()=>void actions.restore(candidate.work.id)}><RotateCcw/>恢复候选</button>
        <button className="icon-action danger-action" aria-label={`移除候选：${candidate.work.title}`} onClick={()=>void actions.remove(candidate.work.id)}><Trash2/></button>
      </>}
      {status==="importing"&&<button disabled><LoaderCircle className="spin"/>正在导入</button>}
      {status==="imported"&&candidate.paper_id&&<button className="primary-action" onClick={()=>onOpenPaper(candidate.paper_id!)}><CheckCircle2/>打开项目论文</button>}
    </div>
  </article>
}

function ResearchEmpty({title,text}:{title:string;text:string}){
  return <div className="project-research-empty"><FileSearch/><h3>{title}</h3><p>{text}</p></div>
}

function searchStateLabel(state:LiteratureSearchRun["state"]){
  return {running:"检索中",completed:"已完成",partial:"部分完成",failed:"失败",cancelled:"已取消"}[state]
}

function formatTime(value:string){
  const date=new Date(value)
  return Number.isNaN(date.getTime())?value:date.toLocaleString()
}

function providerSummary(search:LiteratureSearchRun){
  const providers=Object.entries(search.provider_status)
  if(!providers.length)return "尚无来源结果"
  const hits=providers.reduce((total,[,status])=>total+status.hits,0)
  const failed=providers.filter(([,status])=>status.state==="failed").map(([name])=>name)
  return `${providers.length} 个来源 · ${hits} 条命中${failed.length?` · ${failed.join("、")} 暂不可用`:""}`
}

export function ProjectResearch({project,papers,theme,researchRevision=0,focusWorkId,onFocusHandled,onOpenPaper,onRemovePaper,onChanged}:{project:Project;papers:Paper[];theme:ResolvedTheme;researchRevision?:number;focusWorkId?:string;onFocusHandled?:()=>void;onOpenPaper:(paperId:string)=>void;onRemovePaper:(paperId:string)=>Promise<void>;onChanged?:()=>Promise<void>}){
  const projectId=project.id
  const [tab,setTab]=useState<ProjectResearchTab>("overview")
  const [candidates,setCandidates]=useState<ProjectCandidate[]>([])
  const [searches,setSearches]=useState<LiteratureSearchRun[]>([])
  const [goals,setGoals]=useState<ProjectGoalSummary[]>([])
  const [graph,setGraph]=useState<GraphPayload>({nodes:[],edges:[]})
  const [includeDismissed,setIncludeDismissed]=useState(false)
  const [busy,setBusy]=useState(true)
  const [error,setError]=useState("")
  const [selectedCandidate,setSelectedCandidate]=useState<ProjectCandidate|null>(null)
  const [searchDetail,setSearchDetail]=useState<LiteratureSearchDetail|null>(null)
  const [loadedProjectId,setLoadedProjectId]=useState<string|null>(null)
  const loadedResearchRevision=useRef({projectId,revision:researchRevision})
  const reloadCoordinator=useRef(new ProjectResearchReloadCoordinator())
  const load=useCallback(async()=>{
    return reloadCoordinator.current.run(
      ()=>Promise.all([
        api.projectCandidates(projectId,includeDismissed),
        api.projectLiteratureSearches(projectId),
        api.projectGoals(projectId),
        api.graph({project_id:projectId,include_hypotheses:true}),
      ]),
      ([nextCandidates,nextSearches,nextGoals,nextGraph])=>{
        setCandidates(nextCandidates);setSearches(nextSearches);setGoals(nextGoals);setGraph(nextGraph);setError("");setBusy(false)
        setLoadedProjectId(projectId)
        setSelectedCandidate(current=>current?nextCandidates.find(item=>item.work.id===current.work.id)??null:null)
      },
      value=>{setError(value instanceof Error?value.message:"加载项目候选失败");setBusy(false)},
    )
  },[includeDismissed,projectId])
  useEffect(()=>{reloadCoordinator.current.invalidate();loadedResearchRevision.current={projectId,revision:researchRevision};setTab("overview");setCandidates([]);setSearches([]);setGoals([]);setGraph({nodes:[],edges:[]});setSelectedCandidate(null);setSearchDetail(null);setLoadedProjectId(null)},[projectId])
  useEffect(()=>{setBusy(true);void load()},[load])
  useEffect(()=>{
    const signal={type:"project-research-revised",payload:{project_id:projectId,revision:researchRevision}}
    if(loadedResearchRevision.current.projectId!==projectId){loadedResearchRevision.current={projectId,revision:researchRevision};return}
    if(loadedResearchRevision.current.revision===researchRevision||!shouldReloadProjectResearch(signal,projectId))return
    let cancelled=false
    const reloadRevision=async()=>{
      while(!cancelled&&loadedResearchRevision.current.revision!==researchRevision){
        setBusy(true)
        const applied=await load()
        if(cancelled)return
        if(applied){
          if(loadedResearchRevision.current.projectId===projectId)loadedResearchRevision.current.revision=researchRevision
          return
        }
        await new Promise<void>(resolve=>window.setTimeout(resolve,1200))
      }
    }
    void reloadRevision()
    return()=>{cancelled=true}
  },[load,projectId,researchRevision])
  useEffect(()=>{
    if(!focusWorkId||loadedProjectId!==projectId)return
    const candidate=candidates.find(item=>item.work.id===focusWorkId)
    if(!candidate)return
    setTab("candidates")
    setSelectedCandidate(candidate)
    onFocusHandled?.()
  },[candidates,focusWorkId,loadedProjectId,onFocusHandled,projectId])
  const active=useMemo(()=>goals.some(item=>item.status==="active")||candidates.some(item=>item.status==="importing")||searches.some(item=>item.state==="running"),[candidates,goals,searches])
  useEffect(()=>{if(!active)return;const timer=window.setInterval(()=>void load(),4000);return()=>window.clearInterval(timer)},[active,load])
  const refresh=useCallback(async()=>{await load();await onChanged?.()},[load,onChanged])
  const actions=useMemo(()=>createCandidateActions(projectId,refresh,onOpenPaper),[projectId,refresh,onOpenPaper])
  const openSearch=async(search:LiteratureSearchRun)=>{
    setSearchDetail(await api.literatureSearch(projectId,search.id))
  }
  return <>
    <ProjectResearchView tab={tab} papers={papers} candidates={candidates} searches={searches} includeDismissed={includeDismissed} busy={busy} error={error} actions={actions} onTab={setTab} onOpenCandidate={setSelectedCandidate} onOpenPaper={onOpenPaper} onRemovePaper={paperId=>void onRemovePaper(paperId)} onToggleDismissed={()=>setIncludeDismissed(value=>!value)} onOpenSearch={search=>void openSearch(search)} overview={<ProjectOverview project={project} papers={papers} candidates={candidates} searches={searches} goals={goals} graph={graph} theme={theme} onOpenPaper={onOpenPaper}/>}/>
    {selectedCandidate&&<CandidateDrawer candidate={selectedCandidate} actions={actions} onClose={()=>setSelectedCandidate(null)} onOpenPaper={onOpenPaper}/>}
    {searchDetail&&<SearchDrawer detail={searchDetail} onClose={()=>setSearchDetail(null)}/>}
  </>
}

function CandidateDrawer({candidate,actions,onClose,onOpenPaper}:{candidate:ProjectCandidate;actions:CandidateActions;onClose:()=>void;onOpenPaper:(paperId:string)=>void}){
  return <aside className="research-detail-drawer" role="dialog" aria-modal="false" aria-label="候选论文详情">
    <header><div><span className={`evidence-badge ${candidate.evidence_level}`}>{evidenceLabel(candidate.evidence_level)}</span><h2>{candidate.work.title}</h2></div><button aria-label="关闭候选详情" onClick={onClose}><X/></button></header>
    <dl><div><dt>作者</dt><dd>{candidate.work.authors.join("、")||"未知"}</dd></div><div><dt>年份</dt><dd>{candidate.work.year??"未知"}</dd></div><div><dt>状态</dt><dd>{candidateStatusLabel(candidate.status)}</dd></div><div><dt>发现时间</dt><dd>{formatTime(candidate.created_at)}</dd></div></dl>
    <section><h3>Codex 推荐理由</h3><p>{candidate.relevance_reason}</p></section>
    <section><h3>{candidate.evidence_level==="metadata"?"元数据":"已查证内容"}</h3><p>{candidate.work.abstract_text||"当前只有元数据，尚未取得可核验摘要或全文。"}</p></section>
    <a className="external-source" href={candidate.work.source_url} target="_blank" rel="noreferrer"><ExternalLink/>打开来源</a>
    <footer>
      {candidate.status==="candidate"&&<button className="primary-action" onClick={()=>void actions.importCandidate(candidate.work.id)}>加入项目并分析</button>}
      {candidate.status==="dismissed"&&<button onClick={()=>void actions.restore(candidate.work.id)}><RotateCcw/>恢复候选</button>}
      {candidate.status==="imported"&&candidate.paper_id&&<button className="primary-action" onClick={()=>onOpenPaper(candidate.paper_id!)}><BookOpen/>打开项目论文</button>}
    </footer>
  </aside>
}

function SearchDrawer({detail,onClose}:{detail:LiteratureSearchDetail;onClose:()=>void}){
  return <aside className="research-detail-drawer" role="dialog" aria-modal="false" aria-label="检索详情">
    <header><div><span className={`search-state ${detail.run.state}`}>{searchStateLabel(detail.run.state)}</span><h2>{detail.run.question}</h2></div><button aria-label="关闭检索详情" onClick={onClose}><X/></button></header>
    <p className="search-timing"><Clock3/>{formatTime(detail.run.created_at)} · {detail.run.trigger==="explicit"?"手动检索":"Codex 自动检索"}</p>
    <section><h3>来源状态</h3><ul className="provider-list">{Object.entries(detail.run.provider_status).map(([name,status])=><li key={name}><strong>{name}</strong><span>{status.state==="completed"?`${status.hits} 条命中`:status.error||status.state}</span></li>)}</ul></section>
    <section><h3>全部命中（{detail.results.length}）</h3><ol className="search-result-list">{detail.results.map(result=><li key={result.work.id}><strong>{result.work.title}</strong><span>{result.providers.join("、")} · {result.work.year??"年份未知"}</span></li>)}</ol></section>
  </aside>
}

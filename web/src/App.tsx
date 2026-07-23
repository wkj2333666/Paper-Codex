import { Component, FormEvent, lazy, ReactNode, Suspense, useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react"
import type { CSSProperties } from "react"
import {
  ArchiveRestore, BookOpen, CheckCircle2, ChevronDown, ChevronRight,
  CircleAlert, Database, FileText, Folder, FolderPlus, FolderTree, Inbox, Library,
  Lightbulb, Link2, LoaderCircle, LogOut, Network, Paperclip, Pencil, Plus, Search,
  Send, Sparkles, Trash2, Upload, X,
} from "lucide-react"
import { api, ApiError, session, streamEvents } from "./api"
import { loginErrorMessage } from "./login"
import { buildProjectTree, descendantIds, type ProjectTreeNode } from "./project-tree"
import { briefFromAnalysis, describePaperImpact } from "./reading"
import { initialState, projectPaperCount, reduceEvent } from "./state"
import { groupIntakeTasks, mergeIntakeTaskEvent } from "./intake-status"
import { IntakeTaskCard } from "./IntakeTaskCard"
import { MobilePanelRails, PanelCollapseButton, PanelRail } from "./PanelControls"
import { CodexPanel } from "./CodexPanel"
import { citationsForPaper } from "./citation-overlay"
import type { CodexSelection } from "./conversation-scope"
import { ResizableDivider } from "./ResizableDivider"
import { ThemeToggle } from "./ThemeToggle"
import { cycleThemePreference, readThemePreference, resolveTheme, writeThemePreference, type ResolvedTheme, type ThemePreference } from "./theme"
import {
  PANEL_LIMITS,
  loadPanelLayout,
  resetPanelWidth,
  resizePanel,
  savePanelLayout,
  setPanelOpen,
  type PanelName,
} from "./panel-preferences"
import type { Dashboard, GraphNode, GraphPayload, KnowledgeKind, MessageCitation, Paper, PaperAnalysis, PaperAnnotation, PaperDetail, Project, SearchResult, Task } from "./types"

type Selection=CodexSelection
type Select=(selection:Selection)=>void
const SemanticGraph=lazy(()=>import("./SemanticGraph").then(module=>({default:module.SemanticGraph})))
const PdfDocumentViewer=lazy(()=>import("./PdfDocumentViewer").then(module=>({default:module.PdfDocumentViewer})))

function loadSavedSelection(): Selection {
  try {
    const value = JSON.parse(localStorage.getItem("paper-codex.selection") ?? "null") as Selection | null
    if (value && typeof value.kind === "string") return value
  } catch {}
  return { kind: "workbench" }
}

export default function App(){
  const [authenticated,setAuthenticated]=useState(Boolean(session.get()))
  const [themePreference,setThemePreference]=useState<ThemePreference>(()=>readThemePreference(typeof window === "undefined" ? undefined : window.localStorage))
  const [systemDark,setSystemDark]=useState(()=>typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: dark)").matches)
  const [dashboard,setDashboard]=useState<Dashboard|null>(null)
  const [selection,setSelection]=useState<Selection>(loadSavedSelection)
  const [stream,dispatch]=useReducer(reduceEvent,initialState)
  const lastEvent=useRef(0)
  const [error,setError]=useState("")
  const [panels,setPanels]=useState(loadPanelLayout)
  const [citationFocus,setCitationFocus]=useState<MessageCitation|null>(null)
  const [citationOverlay,setCitationOverlay]=useState<MessageCitation[]>([])
  const updateCitationOverlay=useCallback((next:MessageCitation[])=>setCitationOverlay(current=>current.length===next.length&&current.every((item,index)=>item.id===next[index]?.id&&item.revision===next[index]?.revision)?current:next),[])
  const [activeDrawer,setActiveDrawer]=useState<PanelName|null>(null)
  const resolvedTheme=resolveTheme(themePreference,systemDark)
  const cycleTheme=useCallback(()=>setThemePreference(value=>cycleThemePreference(value)),[])
  const [isNarrow,setIsNarrow]=useState(()=>window.matchMedia("(max-width: 1050px)").matches)
  const drawerTrigger=useRef<HTMLButtonElement|null>(null)
  const closeDrawer=useCallback(()=>{setActiveDrawer(null);requestAnimationFrame(()=>drawerTrigger.current?.focus())},[])
  const openPanel=useCallback((panel:PanelName,trigger:HTMLButtonElement)=>{
    if(isNarrow){drawerTrigger.current=trigger;setActiveDrawer(panel)}
    else setPanels(value=>setPanelOpen(value,panel,true))
  },[isNarrow])
  const collapsePanel=useCallback((panel:PanelName)=>{
    if(isNarrow)closeDrawer()
    else setPanels(value=>setPanelOpen(value,panel,false))
  },[closeDrawer,isNarrow])
  const resize=useCallback((panel:PanelName,delta:number)=>setPanels(value=>resizePanel(value,panel,panel==="sidebar"?delta:-delta,window.innerWidth)),[])
  const reset=useCallback((panel:PanelName)=>setPanels(value=>resetPanelWidth(value,panel)),[])
  const load=useCallback(async()=>{
    try{setDashboard(await api.dashboard());setError("")}
    catch(error){if(error instanceof ApiError&&error.status===401)setAuthenticated(false);else setError(error instanceof Error?error.message:"加载失败")}
  },[])
  useEffect(()=>{if(authenticated)void load()},[authenticated,load])
  useEffect(()=>{document.documentElement.dataset.theme=resolvedTheme;document.documentElement.style.colorScheme=resolvedTheme},[resolvedTheme])
  useEffect(()=>{writeThemePreference(themePreference)},[themePreference])
  useEffect(()=>{
    const media=window.matchMedia("(prefers-color-scheme: dark)")
    const update=()=>setSystemDark(media.matches)
    media.addEventListener("change",update)
    return()=>media.removeEventListener("change",update)
  },[])
  useEffect(()=>{
    if(!authenticated)return
    const controller=new AbortController();let stopped=false
    const run=async()=>{while(!stopped){try{await streamEvents(lastEvent.current,event=>{lastEvent.current=Math.max(lastEvent.current,event.id);dispatch(event);setDashboard(value=>value?{...value,tasks:mergeIntakeTaskEvent(value.tasks,event)}:value);if(["result","failed","done","cancelled"].includes(event.type))void load()},controller.signal)}catch{if(controller.signal.aborted)return;await new Promise(resolve=>setTimeout(resolve,1200))}}}
    void run();return()=>{stopped=true;controller.abort()}
  },[authenticated,load])
  useEffect(()=>savePanelLayout(panels),[panels])
  useEffect(()=>{try{localStorage.setItem("paper-codex.selection",JSON.stringify(selection))}catch{}},[selection])
  useEffect(()=>{if(!dashboard)return;if(selection.kind==="paper"&&selection.id&&!dashboard.papers.some(paper=>paper.id===selection.id))setSelection({kind:"workbench"})},[dashboard,selection.kind,selection.id])
  useEffect(()=>{
    const media=window.matchMedia("(max-width: 1050px)")
    const update=()=>{setIsNarrow(media.matches);if(!media.matches)setActiveDrawer(null)}
    media.addEventListener("change",update)
    return()=>media.removeEventListener("change",update)
  },[])
  useEffect(()=>{
    if(!activeDrawer)return
    const panel=()=>document.querySelector<HTMLElement>(`[data-panel="${activeDrawer}"]`)
    const focusable=()=>Array.from(panel()?.querySelectorAll<HTMLElement>('button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),a[href],[tabindex]:not([tabindex="-1"])')??[])
    const focusFirst=()=>{const first=focusable()[0];if(!first)return false;first.focus();return true}
    const key=(event:KeyboardEvent)=>{
      if(event.key==="Escape"){closeDrawer();return}
      if(event.key==="Tab"){
        const items=focusable();if(!items.length)return
        const first=items[0];const last=items[items.length-1]
        if(event.shiftKey&&document.activeElement===first){event.preventDefault();last.focus()}
        else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus()}
      }
    }
    const containFocus=(event:FocusEvent)=>{const activePanel=panel();if(activePanel&&!activePanel.contains(event.target as Node))focusFirst()}
    const observer=new MutationObserver(()=>{if(focusFirst())observer.disconnect()})
    document.addEventListener("keydown",key)
    document.addEventListener("focusin",containFocus)
    const focusFrame=requestAnimationFrame(()=>{if(!focusFirst())observer.observe(document.body,{childList:true,subtree:true})})
    return()=>{cancelAnimationFrame(focusFrame);observer.disconnect();document.removeEventListener("keydown",key);document.removeEventListener("focusin",containFocus)}
  },[activeDrawer,closeDrawer])
  useEffect(()=>{setActiveDrawer(null);if(citationFocus&&!(selection.kind==="paper"&&selection.id===citationFocus.paper_id))setCitationFocus(null)},[selection.kind,selection.id,citationFocus])
  if(!authenticated)return <Login onLogin={()=>setAuthenticated(true)}/>
  if(!dashboard)return <div className="boot"><LoaderCircle className="spin"/>正在打开研究工作区…</div>
  const logout=()=>{session.clear();setAuthenticated(false)}
  const graphMode=selection.kind==="graph"
  const codexScopeLabel=selection.kind==="paper"&&selection.id
    ?dashboard.papers.find(paper=>paper.id===selection.id)?.title??selection.id
    :selection.kind==="project"&&selection.id
      ?dashboard.projects.find(project=>project.id===selection.id)?.name??selection.id
      :"全部论文"
  const shellStyle={
    "--sidebar-width":`${panels.widths.sidebar}px`,
    "--paper-graph-width":`${panels.widths.paperGraph}px`,
    "--codex-width":`${panels.widths.codex}px`,
    "--sidebar-divider-width":!isNarrow&&panels.sidebarOpen?"6px":"0px",
    "--codex-divider-width":!isNarrow&&!graphMode&&panels.codexOpen?"6px":"0px",
  } as CSSProperties
  const shellClass=["app-shell",graphMode&&"graph-mode",!isNarrow&&!panels.sidebarOpen&&"sidebar-collapsed",!isNarrow&&!panels.codexOpen&&"codex-collapsed",activeDrawer&&"drawer-active"].filter(Boolean).join(" ")
  return <div className={shellClass} style={shellStyle}>
    {(isNarrow||panels.sidebarOpen)
      ?<Sidebar dashboard={dashboard} selection={selection} select={setSelection} refresh={load} logout={logout} drawerOpen={activeDrawer==="sidebar"} onCollapse={()=>collapsePanel("sidebar")} themePreference={themePreference} resolvedTheme={resolvedTheme} onCycleTheme={cycleTheme}/>
      :<PanelRail panel="sidebar" label="文件树" side="left" onExpand={trigger=>openPanel("sidebar",trigger)}/>}
    {!isNarrow&&panels.sidebarOpen&&
      <ResizableDivider panel="sidebar" value={panels.widths.sidebar} min={PANEL_LIMITS.sidebar[0]} max={PANEL_LIMITS.sidebar[1]} onResize={delta=>resize("sidebar",delta)} onReset={()=>reset("sidebar")}/>
    }
    <main className="main-pane">
      {error&&<div className="error-banner"><CircleAlert size={17}/>{error}</div>}
      <MainView dashboard={dashboard} selection={selection} select={setSelection} refresh={load} citationOverlay={citationOverlay} citationFocus={citationFocus} paperGraphOpen={panels.paperGraphOpen} paperGraphWidth={panels.widths.paperGraph} isNarrow={isNarrow} activeDrawer={activeDrawer} openPanel={openPanel} collapsePanel={collapsePanel} resizePanel={resize} resetPanel={reset} theme={resolvedTheme}/>
    </main>
    {!graphMode&&!isNarrow&&panels.codexOpen&&
      <ResizableDivider panel="codex" value={panels.widths.codex} min={PANEL_LIMITS.codex[0]} max={PANEL_LIMITS.codex[1]} onResize={delta=>resize("codex",delta)} onReset={()=>reset("codex")}/>
    }
    {!graphMode&&((isNarrow||panels.codexOpen)
      ?<CodexPanel selection={selection} scopeLabel={codexScopeLabel} activities={stream.activities} drawerOpen={activeDrawer==="codex"} onCollapse={()=>collapsePanel("codex")} onCitation={citation=>{setCitationFocus(citation);setSelection({kind:"paper",id:citation.paper_id})}} onCitations={updateCitationOverlay} onSelect={setSelection}/>
      :<PanelRail panel="codex" label="Codex" side="right" onExpand={trigger=>openPanel("codex",trigger)}/>)}
    {isNarrow&&<MobilePanelRails showPaperGraph={selection.kind==="paper"} showCodex={!graphMode} onOpen={openPanel}/>}
    {activeDrawer&&<button type="button" className="drawer-backdrop" aria-label="关闭面板" onClick={closeDrawer}/>}
  </div>
}

function Login({onLogin}:{onLogin:()=>void}){
  const [password,setPassword]=useState("");const [error,setError]=useState("");const [busy,setBusy]=useState(false)
  const submit=async(event:FormEvent)=>{event.preventDefault();setBusy(true);setError("");try{await api.login(password);onLogin()}catch(error){setError(loginErrorMessage(error))}finally{setBusy(false)}}
  return <div className="login-page"><div className="login-card"><div className="mark"><BookOpen/></div><p className="eyebrow">私人论文研究工作区</p><h1>Paper Codex</h1><p className="muted">让论文彼此连接，而不只是堆在文件夹里。</p><form onSubmit={submit}><label>工作区密码</label><input autoFocus type="password" value={password} onChange={event=>setPassword(event.target.value)} placeholder="••••••••"/>{error&&<p className="form-error">{error}</p>}<button className="primary" disabled={busy}>{busy?<LoaderCircle className="spin" size={17}/>:<Sparkles size={17}/>}进入工作区</button></form></div></div>
}

function Sidebar({dashboard,selection,select,refresh,logout,drawerOpen,onCollapse,themePreference,resolvedTheme,onCycleTheme}:{dashboard:Dashboard;selection:Selection;select:Select;refresh:()=>Promise<void>;logout:()=>void;drawerOpen:boolean;onCollapse:()=>void;themePreference:ThemePreference;resolvedTheme:ResolvedTheme;onCycleTheme:()=>void}){
  const [creating,setCreating]=useState(false);const [name,setName]=useState("");const [purpose,setPurpose]=useState("");const [parentId,setParentId]=useState<string|null>(null);const [projectQuery,setProjectQuery]=useState("")
  const tree=useMemo(()=>buildProjectTree(dashboard.projects,dashboard.project_memberships),[dashboard.projects,dashboard.project_memberships])
  const create=async(event:FormEvent)=>{event.preventDefault();const project=await api.createProject(name,purpose,parentId);setCreating(false);setName("");setPurpose("");setParentId(null);await refresh();select({kind:"project",id:project.id})}
  const move=async(sourceId:string,parent_id:string|null)=>{const source=dashboard.projects.find(project=>project.id===sourceId);if(!source||source.id===parent_id||descendantIds(dashboard.projects,source.id).has(parent_id??""))return;await api.updateProject(source.id,{name:source.name,purpose:source.purpose,parent_id});await refresh()}
  const rename=async(project:Project)=>{const next=window.prompt("新的项目名称",project.name)?.trim();if(!next)return;await api.updateProject(project.id,{name:next,purpose:project.purpose,parent_id:project.parent_id});await refresh()}
  const remove=async(project:Project)=>{const impact=await api.projectImpact(project.id);if(!window.confirm(`删除“${project.name}”？\n${impact.direct_papers} 篇直接论文将回到收件箱，${impact.descendant_projects} 个子项目会上移。论文不会被删除。`))return;await api.deleteProject(project.id);await refresh();select({kind:"workbench"})}
  return <aside className={`sidebar workspace-panel${drawerOpen?" drawer-open":""}`} data-panel="sidebar"><div className="brand"><div className="brand-icon"><BookOpen size={19}/></div><div><strong>Paper Codex</strong><span>论文研究记忆</span></div><PanelCollapseButton label="文件树" direction="left" onCollapse={onCollapse}/></div>
    <nav>
      <Nav active={selection.kind==="workbench"} icon={<Sparkles/>} label="工作台" onClick={()=>select({kind:"workbench"})}/>
      <Nav active={selection.kind==="inbox"} icon={<Inbox/>} label="收件箱" badge={dashboard.inbox.length} onClick={()=>select({kind:"inbox"})}/>
      <Nav active={selection.kind==="graph"} icon={<Network/>} label="知识图谱" onClick={()=>select({kind:"graph"})}/>
      <Nav active={selection.kind==="search"} icon={<Search/>} label="全文检索" onClick={()=>select({kind:"search"})}/>
      <Nav active={selection.kind==="trash"} icon={<Trash2/>} label="回收站" badge={dashboard.trash_count} onClick={()=>select({kind:"trash"})}/>
    </nav>
    <div className="section-title"><span>研究项目</span><button onClick={()=>setCreating(value=>!value)} title="新建项目"><FolderPlus size={15}/></button></div>
    <div className="project-search"><Search/><input value={projectQuery} onChange={event=>setProjectQuery(event.target.value)} placeholder="查找项目"/></div>
    {creating&&<form className="new-project" onSubmit={create}><input autoFocus value={name} onChange={event=>setName(event.target.value)} placeholder="项目名称" required/><textarea value={purpose} onChange={event=>setPurpose(event.target.value)} placeholder="研究目标（可选）"/><select value={parentId??""} onChange={event=>setParentId(event.target.value||null)}><option value="">顶层项目</option>{dashboard.projects.map(project=><option key={project.id} value={project.id}>{project.name}</option>)}</select><button className="small-primary">创建项目</button></form>}
    <div className="project-list" onDragOver={event=>event.preventDefault()} onDrop={event=>{event.preventDefault();const id=event.dataTransfer.getData("text/project-id");if(id)void move(id,null)}}>
      {tree.length?tree.map(node=><ProjectTreeRow key={node.id} node={node} query={projectQuery} selected={selection.kind==="project"?selection.id:undefined} select={select} move={move} rename={rename} remove={remove}/>):<div className="empty-small">还没有项目</div>}
    </div>
    <div className="sidebar-foot"><span>{dashboard.papers.length} 篇论文</span><div className="sidebar-actions"><ThemeToggle preference={themePreference} resolvedTheme={resolvedTheme} onCycle={onCycleTheme}/><button onClick={logout}><LogOut size={14}/>退出</button></div></div>
  </aside>
}

function ProjectTreeRow({node,query,selected,select,move,rename,remove,depth=0}:{node:ProjectTreeNode;query:string;selected?:string;select:Select;move:(source:string,parent:string|null)=>Promise<void>;rename:(project:Project)=>Promise<void>;remove:(project:Project)=>Promise<void>;depth?:number}){
  const [open,setOpen]=useState(true)
  const matches=!query.trim()||node.name.toLowerCase().includes(query.trim().toLowerCase())||node.children.some(child=>treeContains(child,query))
  if(!matches)return null
  return <div className="tree-node"><div className={selected===node.id?"project-row active":"project-row"} style={{paddingLeft:8+depth*15}} draggable onDragStart={event=>event.dataTransfer.setData("text/project-id",node.id)} onDragOver={event=>event.preventDefault()} onDrop={event=>{event.preventDefault();event.stopPropagation();const source=event.dataTransfer.getData("text/project-id");if(source)void move(source,node.id)}}>
    <button className="tree-toggle" onClick={()=>setOpen(value=>!value)} disabled={!node.children.length}>{node.children.length?(open?<ChevronDown/>:<ChevronRight/>):<span/>}</button>
    <button className="tree-main" onClick={()=>select({kind:"project",id:node.id})}><Folder/><span>{node.name}</span><em>{node.paperCount}</em></button>
    <div className="tree-actions"><button title="重命名" onClick={()=>void rename(node)}><Pencil/></button><button title="删除项目" onClick={()=>void remove(node)}><Trash2/></button></div>
  </div>{open&&node.children.map(child=><ProjectTreeRow key={child.id} node={child} query={query} selected={selected} select={select} move={move} rename={rename} remove={remove} depth={depth+1}/>)}</div>
}
function treeContains(node:ProjectTreeNode,query:string):boolean{return node.name.toLowerCase().includes(query.trim().toLowerCase())||node.children.some(child=>treeContains(child,query))}
function Nav({active,icon,label,badge,onClick}:{active:boolean;icon:ReactNode;label:string;badge?:number;onClick:()=>void}){return <button className={active?"nav-row active":"nav-row"} onClick={onClick}>{icon}<span>{label}</span>{badge!==undefined&&<em>{badge}</em>}</button>}

function MainView({dashboard,selection,select,refresh,citationOverlay,citationFocus,paperGraphOpen,paperGraphWidth,isNarrow,activeDrawer,openPanel,collapsePanel,resizePanel,resetPanel,theme}:{dashboard:Dashboard;selection:Selection;select:Select;refresh:()=>Promise<void>;citationOverlay:MessageCitation[];citationFocus:MessageCitation|null;paperGraphOpen:boolean;paperGraphWidth:number;isNarrow:boolean;activeDrawer:PanelName|null;openPanel:(panel:PanelName,trigger:HTMLButtonElement)=>void;collapsePanel:(panel:PanelName)=>void;resizePanel:(panel:PanelName,delta:number)=>void;resetPanel:(panel:PanelName)=>void;theme:ResolvedTheme}){
  const paperCitations=useMemo(()=>selection.kind==="paper"&&selection.id?citationsForPaper(citationOverlay,selection.id):[],[citationOverlay,selection.kind,selection.id])
  if(selection.kind==="paper"&&selection.id)return <PaperView id={selection.id} dashboard={dashboard} select={select} refresh={refresh} citations={paperCitations} citationFocus={citationFocus?.paper_id===selection.id?citationFocus:null} paperGraphOpen={paperGraphOpen} paperGraphWidth={paperGraphWidth} isNarrow={isNarrow} drawerOpen={activeDrawer==="paperGraph"} onExpandGraph={trigger=>openPanel("paperGraph",trigger)} onCollapseGraph={()=>collapsePanel("paperGraph")} onResizeGraph={delta=>resizePanel("paperGraph",delta)} onResetGraph={()=>resetPanel("paperGraph")} theme={theme}/>
  if(selection.kind==="project"&&selection.id)return <ProjectView project={dashboard.projects.find(project=>project.id===selection.id)} dashboard={dashboard} select={select} refresh={refresh}/>
  if(selection.kind==="inbox")return <PaperGrid title="收件箱" subtitle="尚未归入研究项目的论文" papers={dashboard.inbox} select={select}/>
  if(selection.kind==="search")return <SearchView select={select}/>
  if(selection.kind==="graph")return <GraphWorkspace dashboard={dashboard} focusNode={selection.id} select={select} theme={theme}/>
  if(selection.kind==="trash")return <TrashView select={select} refresh={refresh}/>
  return <Workbench dashboard={dashboard} select={select} refresh={refresh}/>
}

export function Workbench({dashboard,select,refresh}:{dashboard:Dashboard;select:Select;refresh:()=>Promise<void>}){
  const [source,setSource]=useState("");const [project,setProject]=useState("");const [busy,setBusy]=useState(false);const fileRef=useRef<HTMLInputElement>(null)
  const submit=async(event:FormEvent)=>{event.preventDefault();if(!source.trim())return;setBusy(true);try{await api.intake(source,project||undefined);await refresh();setSource("")}finally{setBusy(false)}}
  const upload=async(file?:File)=>{if(!file)return;setBusy(true);try{await api.upload(file,project||undefined);await refresh()}finally{setBusy(false)}}
  const recent=dashboard.papers.slice(0,6)
  const intakeTasks=groupIntakeTasks(dashboard.tasks)
  const recentFailures=intakeTasks.failed.slice(0,6)
  const cancelTask=async(id:string)=>{await api.cancelTask(id);await refresh()}
  const dismissTask=async(id:string)=>{await api.dismissTask(id);await refresh()}
  const clearFailures=async()=>{try{await Promise.all(intakeTasks.failed.map(task=>api.dismissTask(task.id)))}finally{await refresh()}}
  return <div className="content-wrap"><header className="hero"><p className="eyebrow">论文优先的研究工作流</p><h1>今天想读什么？</h1><p>输入论文名称、DOI、arXiv 或链接。Codex 会用中文整理证据，并把方法、概念和发现接入知识图谱。</p></header>
    <form className="intake-card" onSubmit={submit}><div className="intake-line"><Sparkles/><input value={source} onChange={event=>setSource(event.target.value)} placeholder="粘贴论文名称、链接、DOI 或 arXiv…"/><button disabled={busy||!source.trim()}>{busy?<LoaderCircle className="spin"/>:<Send/>}</button></div><div className="intake-options"><select value={project} onChange={event=>setProject(event.target.value)}><option value="">暂不归类</option>{dashboard.projects.map(project=><option key={project.id} value={project.id}>{project.name}</option>)}</select><input ref={fileRef} hidden type="file" accept="application/pdf" onChange={event=>void upload(event.target.files?.[0])}/><button type="button" className="ghost" onClick={()=>fileRef.current?.click()}><Paperclip/>上传 PDF</button></div></form>
    <div className="stats"><div><Library/><strong>{dashboard.papers.length}</strong><span>论文</span></div><div><FolderTree/><strong>{dashboard.projects.length}</strong><span>研究项目</span></div><div><Network/><strong>{dashboard.papers.filter(paper=>paper.note_path).length}</strong><span>已结构化</span></div></div>
    {intakeTasks.active.length>0&&<><SectionHead title="正在处理"/><div className="paper-grid intake-task-grid">{intakeTasks.active.map(task=><IntakeTaskCard key={task.id} task={task} onCancel={id=>void cancelTask(id)} onDismiss={id=>void dismissTask(id)}/>)}</div></>}
    {recentFailures.length>0&&<><SectionHead title="最近失败" action="清除失败记录" onClick={()=>void clearFailures()}/><div className="paper-grid intake-task-grid">{recentFailures.map(task=><IntakeTaskCard key={task.id} task={task} onCancel={id=>void cancelTask(id)} onDismiss={id=>void dismissTask(id)}/>)}</div></>}
    <SectionHead title="最近论文" action={dashboard.papers.length>6?"查看全部":undefined} onClick={()=>select({kind:"inbox"})}/>{recent.length?<div className="paper-grid">{recent.map(paper=><PaperCard key={paper.id} paper={paper} onClick={()=>select({kind:"paper",id:paper.id})}/>)}</div>:<Empty icon={<Upload/>} title="论文库还是空的" text="从上方输入一篇论文开始。"/>}
  </div>
}

function ProjectView({project,dashboard,select,refresh}:{project?:Project;dashboard:Dashboard;select:Select;refresh:()=>Promise<void>}){
  if(!project)return <Empty icon={<Folder/>} title="项目不存在" text="请重新选择。"/>
  const ids=dashboard.project_memberships[project.id]??[];const papers=dashboard.papers.filter(paper=>ids.includes(paper.id));const children=dashboard.projects.filter(item=>item.parent_id===project.id)
  const rename=async()=>{const name=window.prompt("新的项目名称",project.name)?.trim();if(!name)return;await api.updateProject(project.id,{name,purpose:project.purpose,parent_id:project.parent_id});await refresh()}
  const remove=async(subtree:boolean)=>{const impact=await api.projectImpact(project.id);const message=subtree?`删除“${project.name}”及 ${impact.descendant_projects} 个子项目？论文只会失去项目引用，不会被删除。`:`删除“${project.name}”？${impact.descendant_projects} 个子项目将上移，论文不会被删除。`;if(!window.confirm(message))return;await api.deleteProject(project.id,subtree);await refresh();select({kind:"workbench"})}
  const removePaper=async(paperId:string)=>{await api.removePaper(project.id,paperId);await refresh()}
  return <div className="content-wrap"><header className="project-hero"><div className="folder-large"><Folder/></div><div><p className="eyebrow">研究项目</p><h1>{project.name}</h1><p>{project.purpose||"尚未写下研究目标"}</p></div><div className="project-actions"><button onClick={()=>void rename()}><Pencil/>重命名</button><button onClick={()=>void remove(false)}><Trash2/>删除并上移子项目</button>{children.length>0&&<button className="danger" onClick={()=>void remove(true)}><Trash2/>删除整个子树</button>}</div></header><div className="project-meta"><span>{papers.length} 篇直接论文</span><span>{children.length} 个子项目</span><span>论文可被多个项目引用</span></div><SectionHead title="项目论文"/>{papers.length?<div className="paper-grid">{papers.map(paper=><PaperCard key={paper.id} paper={paper} onClick={()=>select({kind:"paper",id:paper.id})} remove={()=>void removePaper(paper.id)}/>)}</div>:<Empty icon={<Plus/>} title="这个项目还是空的" text="导入论文时选择本项目，或在论文页添加。"/>}</div>
}

function PaperView({id,dashboard,select,refresh,citations,citationFocus,paperGraphOpen,paperGraphWidth,isNarrow,drawerOpen,onExpandGraph,onCollapseGraph,onResizeGraph,onResetGraph,theme}:{id:string;dashboard:Dashboard;select:Select;refresh:()=>Promise<void>;citations:MessageCitation[];citationFocus:MessageCitation|null;paperGraphOpen:boolean;paperGraphWidth:number;isNarrow:boolean;drawerOpen:boolean;onExpandGraph:(trigger:HTMLButtonElement)=>void;onCollapseGraph:()=>void;onResizeGraph:(delta:number)=>void;onResetGraph:()=>void;theme:ResolvedTheme}){
  const [detail,setDetail]=useState<PaperDetail|null>(null);const [graph,setGraph]=useState<GraphPayload>({nodes:[],edges:[]});const [annotations,setAnnotations]=useState<PaperAnnotation[]>([]);const [focusedCitation,setFocusedCitation]=useState<MessageCitation|null>(null);const [project,setProject]=useState("");const [tab,setTab]=useState("overview");const [readerMode,setReaderMode]=useState<"smart"|"enhanced"|"original">("smart");const [reanalyzing,setReanalyzing]=useState(false)
  const citationKey=citations.map(citation=>citation.id).join("|")
  const reload=useCallback(async()=>{const [paper,graph,annotations]=await Promise.all([api.paper(id),api.graph({paper_id:id}),api.paperAnnotations(id)]);setDetail(paper);setGraph(graph);setAnnotations(annotations)},[id])
  useEffect(()=>{setDetail(null);setAnnotations([]);setFocusedCitation(null);void reload()},[reload])
  useEffect(()=>{if(citationFocus){setFocusedCitation(citationFocus);setReaderMode("enhanced")}},[citationFocus])
  useEffect(()=>{if(citations.length)setReaderMode("enhanced")},[citationKey])
  if(!detail)return <div className="boot"><LoaderCircle className="spin"/>加载论文…</div>
  let authors:string[]=[];try{authors=JSON.parse(detail.paper.authors_json)}catch{}
  const brief=briefFromAnalysis(detail.analysis)
  const add=async()=>{if(!project)return;await api.addPaper(project,id);setProject("");await reload();await refresh()}
  const removeMembership=async(projectId:string)=>{await api.removePaper(projectId,id);await reload();await refresh()}
  const openPdf=()=>setReaderMode("original")
  const reanalyze=async()=>{const source=detail.paper.source_url??(detail.paper.arxiv_id?`arxiv:${detail.paper.arxiv_id}`:detail.paper.doi?`doi:${detail.paper.doi}`:detail.paper.title);setReanalyzing(true);try{await api.intake(source,detail.projects[0])}finally{setReanalyzing(false)}}
  const trash=async()=>{const impact=await api.paperImpact(id);if(!window.confirm(`把这篇论文移入回收站？\n${describePaperImpact(impact)}。PDF、笔记和关系会保留，可随时恢复。`))return;await api.trashPaper(id);await refresh();select({kind:"inbox"})}
  const selectedAnnotation=annotations.find(item=>item.citation.id===focusedCitation?.id)
  const pinActive=async(citation:MessageCitation=focusedCitation as MessageCitation)=>{if(!citation)return;await api.pinCitation(citation.id);await reload()}
  const hideActive=async(citation:MessageCitation=focusedCitation as MessageCitation)=>{const annotation=annotations.find(item=>item.citation.id===citation?.id);if(!annotation)return;await api.updateAnnotation(annotation.annotation.id,"hidden");setAnnotations(value=>value.map(item=>item.annotation.id===annotation.annotation.id?{...item,annotation:{...item.annotation,state:"hidden"}}:item));if(focusedCitation?.id===citation.id)setFocusedCitation(null);await reload()}
  const visibleAnnotations=annotations.filter(item=>item.annotation.state==="visible")
  const available=dashboard.projects.filter(item=>!detail.projects.includes(item.id))
  return <div className={`paper-page${!isNarrow&&!paperGraphOpen?" paper-graph-collapsed":""}`}><div className="paper-reading"><header className="paper-head"><div><p className="eyebrow">{detail.paper.year??"论文"} · {detail.paper.doi??detail.paper.arxiv_id??detail.paper.id}</p><h1>{detail.paper.title}</h1><p>{authors.join(", ")||"作者信息待补充"}</p></div><div className="paper-head-actions"><button className="outline" onClick={openPdf}><FileText/>阅读原文</button><button className="outline" disabled={reanalyzing} onClick={()=>void reanalyze()}>{reanalyzing?<LoaderCircle className="spin"/>:<Sparkles/>}重新分析</button><button className="danger-outline" onClick={()=>void trash()}><Trash2/>移入回收站</button></div></header>
    <div className="reader-mode-tabs">{[["smart","智能阅读"],["enhanced","增强阅读"],["original","原文"]].map(([key,label])=><button key={key} className={readerMode===key?"active":""} onClick={()=>setReaderMode(key as typeof readerMode)}>{label}</button>)}</div>
    {readerMode==="smart"?<>
    <div className="takeaway"><span>一句话读懂</span><strong>{brief.takeaway}</strong></div>
    <div className="brief-grid"><BriefCard icon={<Lightbulb/>} title="研究问题" values={brief.researchQuestion}/><BriefCard icon={<Sparkles/>} title="核心方法" values={brief.method}/><BriefCard icon={<CheckCircle2/>} title="关键结果" values={brief.results}/><BriefCard icon={<CircleAlert/>} title="主要局限" values={brief.limitations}/></div>
    <div className="paper-toolbar"><div className="membership-chips">{detail.projects.map(projectId=>{const item=dashboard.projects.find(project=>project.id===projectId);return item&&<span key={projectId}>{item.name}<button title="移出项目" onClick={()=>void removeMembership(projectId)}><X/></button></span>})}</div><select value={project} onChange={event=>setProject(event.target.value)}><option value="">添加到项目…</option>{available.map(item=><option key={item.id} value={item.id}>{item.name}</option>)}</select><button onClick={()=>void add()} disabled={!project}>添加</button></div>
    <div className="reading-tabs">{[["overview","概要"],["method","方法与实验"],["results","结果"],["limitations","局限与复现"],["evidence","证据"]].map(([key,label])=><button key={key} className={tab===key?"active":""} onClick={()=>setTab(key)}>{label}</button>)}</div><AnalysisTab tab={tab} analysis={detail.analysis}/></>:<>{readerMode==="enhanced"&&visibleAnnotations.length>0&&<div className="annotation-browser"><strong>已固定说明</strong>{visibleAnnotations.map((item,index)=><button className={item.citation.id===focusedCitation?.id?"active":""} key={item.annotation.id} onClick={()=>{setFocusedCitation(item.citation);setReaderMode("enhanced")}}>#{index+1} · 第 {item.citation.page} 页</button>)}</div>}<Suspense fallback={<div className="pdf-loading"><LoaderCircle className="spin"/>正在加载阅读器…</div>}><PdfDocumentViewer paperId={id} theme={theme} className={readerMode==="enhanced"?"enhanced-reader":"original-reader"} citations={readerMode==="enhanced"?citations:[]} annotations={readerMode==="enhanced"?annotations:[]} focusedCitationId={readerMode==="enhanced"?focusedCitation?.id:null} currentRevision={detail.paper.canonical_sha256} onPin={citation=>{setFocusedCitation(citation);void pinActive(citation)}} onHide={citation=>void hideActive(citation)}/></Suspense></>}
  </div>{!isNarrow&&paperGraphOpen&&<ResizableDivider panel="paperGraph" value={paperGraphWidth} min={PANEL_LIMITS.paperGraph[0]} max={PANEL_LIMITS.paperGraph[1]} onResize={onResizeGraph} onReset={onResetGraph}/>} {!isNarrow&&!paperGraphOpen
    ?<PanelRail panel="paperGraph" label="相关知识" side="right" className="paper-graph-rail" onExpand={onExpandGraph}/>
    :<aside className={`paper-graph workspace-panel${drawerOpen?" drawer-open":""}`} data-panel="paperGraph"><div className="paper-graph-head"><div><p className="eyebrow">相关知识</p><h2>这篇论文连接了什么？</h2><p>实线表示有论文证据，弱化关系表示 Codex 的待验证假设。</p></div><PanelCollapseButton label="相关知识" direction="right" onCollapse={onCollapseGraph}/></div><GraphErrorBoundary><Suspense fallback={<GraphLoading/>}><SemanticGraph theme={theme} compact payload={graph} focusNode={id} onOpenFull={()=>select({kind:"graph",id})}/></Suspense></GraphErrorBoundary></aside>}</div>
}

function BriefCard({icon,title,values}:{icon:ReactNode;title:string;values:string[]}){return <section className="brief-card"><div>{icon}<h3>{title}</h3></div><ul>{values.map((value,index)=><li key={index}>{value}</li>)}</ul></section>}
function AnalysisTab({tab,analysis}:{tab:string;analysis:PaperAnalysis|null}){
  const value=analysis??{};const list=(item:unknown)=>Array.isArray(item)?item.map(String):typeof item==="string"?item.split(/\n+/).filter(Boolean):[];const text=(item:unknown)=>typeof item==="string"&&item.trim()?item:"尚待 Codex 补充"
  if(tab==="method")return <div className="analysis-panel"><AnalysisSection title="方法" body={text(value.method)}/><AnalysisSection title="实验设计" body={text(value.experimental_design)}/><AnalysisList title="对比基线" values={list(value.baselines)}/></div>
  if(tab==="results")return <div className="analysis-panel"><AnalysisList title="关键结果" values={list(value.results)}/></div>
  if(tab==="limitations")return <div className="analysis-panel"><AnalysisList title="局限" values={list(value.limitations)}/><AnalysisList title="前提与假设" values={list(value.assumptions)}/><AnalysisSection title="可复现性" body={text(value.reproducibility)}/></div>
  if(tab==="evidence"){const evidence=Array.isArray(value.evidence)?value.evidence:[];return <div className="evidence-list">{evidence.length?evidence.map((item,index)=><article key={index}><strong>第 {item.page} 页</strong><span>{item.section||"未标章节"}{item.locator?` · ${item.locator}`:""}</span><em>{item.kind}</em></article>):<Empty icon={<Link2/>} title="旧笔记缺少可展示的证据定位" text="下次重新分析后会补充页码、章节、图表等证据。"/>}</div>}
  return <div className="analysis-panel"><AnalysisSection title="研究问题" body={text(value.research_question)}/><AnalysisSection title="核心贡献" body={text(value.contribution)}/></div>
}
function AnalysisSection({title,body}:{title:string;body:string}){return <section><h3>{title}</h3><p>{body}</p></section>}
function AnalysisList({title,values}:{title:string;values:string[]}){return <section><h3>{title}</h3><ul>{(values.length?values:["尚待 Codex 补充"]).map((value,index)=><li key={index}>{value}</li>)}</ul></section>}

function GraphWorkspace({dashboard,focusNode,select,theme}:{dashboard:Dashboard;focusNode?:string;select:Select;theme:ResolvedTheme}){
  const allKinds:KnowledgeKind[]=["paper","concept","method","dataset","finding"]
  const [project,setProject]=useState("");const [kinds,setKinds]=useState(new Set(allKinds));const [hypotheses,setHypotheses]=useState(true);const [graph,setGraph]=useState<GraphPayload>({nodes:[],edges:[]});const [selected,setSelected]=useState<GraphNode|null>(null);const [busy,setBusy]=useState(true)
  useEffect(()=>{let active=true;setBusy(true);void api.graph({project_id:project||undefined,kinds:[...kinds],include_hypotheses:hypotheses}).then(value=>{if(active)setGraph(value)}).finally(()=>{if(active)setBusy(false)});return()=>{active=false}},[project,kinds,hypotheses])
  const counts=useMemo(()=>Object.fromEntries(allKinds.map(kind=>[kind,graph.nodes.filter(node=>node.kind===kind).length])),[graph.nodes])
  const degrees=useMemo(()=>{const map=new Map<string,number>();for(const edge of graph.edges){map.set(edge.source,(map.get(edge.source)??0)+1);map.set(edge.target,(map.get(edge.target)??0)+1)}return [...map.entries()].sort((a,b)=>b[1]-a[1]).slice(0,5).map(([id,count])=>({node:graph.nodes.find(node=>node.id===id),count})).filter(item=>item.node)},[graph])
  const toggle=(kind:KnowledgeKind)=>setKinds(current=>{const next=new Set(current);next.has(kind)?next.delete(kind):next.add(kind);return next})
  return <div className="graph-workspace"><aside className="graph-filters"><p className="eyebrow">研究关系网络</p><h1>知识图谱</h1><p>按项目查看论文、方法、概念、数据集与研究发现之间的联系。</p><label>项目范围<select value={project} onChange={event=>setProject(event.target.value)}><option value="">全部论文</option>{dashboard.projects.map(item=><option key={item.id} value={item.id}>{item.name}</option>)}</select></label><fieldset><legend>节点类型</legend>{allKinds.map(kind=><label key={kind}><input type="checkbox" checked={kinds.has(kind)} onChange={()=>toggle(kind)}/><span className={`kind-dot ${kind}`}/>{kindName(kind)}<em>{counts[kind]??0}</em></label>)}</fieldset><label className="hypothesis-toggle"><input type="checkbox" checked={hypotheses} onChange={event=>setHypotheses(event.target.checked)}/>显示 Codex 假设关系</label><div className="edge-key"><span><i className="edge-solid"/>证据支持</span><span><i className="edge-dashed"/>待验证假设</span></div></aside>
    <section className="graph-stage">{busy?<GraphLoading/>:<GraphErrorBoundary><Suspense fallback={<GraphLoading/>}><SemanticGraph theme={theme} payload={graph} focusNode={focusNode} onSelectionChange={setSelected} onPaperOpen={paperId=>select({kind:"paper",id:paperId})}/></Suspense></GraphErrorBoundary>}</section>
    <aside className="graph-insights"><p className="eyebrow">图谱洞察</p>{selected?<><h2>{selected.label}</h2><span className="node-kind">{kindName(selected.kind)}</span><p>{selected.description||"暂无补充说明"}</p>{selected.paper_id&&<button onClick={()=>select({kind:"paper",id:selected.paper_id!})}>打开论文<ChevronRight/></button>}</>:<><h2>结构概览</h2><div className="insight-stats"><div><strong>{graph.nodes.length}</strong><span>节点</span></div><div><strong>{graph.edges.length}</strong><span>关系</span></div><div><strong>{graph.edges.filter(edge=>edge.hypothesis).length}</strong><span>假设</span></div></div><h3>关键连接点</h3><div className="hub-list">{degrees.map(({node,count})=><button key={node!.id} onClick={()=>setSelected(node!)}><span>{node!.label}</span><em>{count} 条连接</em></button>)}</div></>}</aside>
  </div>
}
function kindName(kind:KnowledgeKind){return {paper:"论文",concept:"概念",method:"方法",dataset:"数据集",finding:"研究发现"}[kind]}
function GraphLoading(){return <div className="boot"><LoaderCircle className="spin"/>加载图谱…</div>}
class GraphErrorBoundary extends Component<{children:ReactNode},{failed:boolean}>{
  state={failed:false}
  static getDerivedStateFromError(){return {failed:true}}
  render(){return this.state.failed?<div className="graph-fallback"><Network/><h3>当前浏览器无法显示交互图谱</h3><p>请启用 WebGL 或更换现代浏览器。论文内容、项目筛选和右侧图谱洞察仍可正常使用。</p></div>:this.props.children}
}

function TrashView({select,refresh}:{select:Select;refresh:()=>Promise<void>}){
  const [papers,setPapers]=useState<Paper[]|null>(null);const load=useCallback(()=>api.trash().then(setPapers),[]);useEffect(()=>{void load()},[load])
  if(!papers)return <div className="boot"><LoaderCircle className="spin"/>加载回收站…</div>
  const restore=async(id:string)=>{await api.restorePaper(id);await load();await refresh()}
  const destroy=async(paper:Paper)=>{const impact=await api.paperImpact(paper.id);if(window.prompt(`永久删除将移除 PDF、笔记和全部关系。\n${describePaperImpact(impact)}。\n输入“永久删除”继续：`)!=="永久删除")return;await api.permanentlyDeletePaper(paper.id);await load();await refresh()}
  return <div className="content-wrap"><header className="page-head"><p className="eyebrow">可恢复删除</p><h1>回收站</h1><p>这里的论文不会出现在项目、检索或图谱中。恢复不会丢失笔记和关系。</p></header>{papers.length?<div className="trash-list">{papers.map(paper=><article key={paper.id}><div><FileText/><div><h3>{paper.title}</h3><p>{paper.deleted_at?`删除于 ${new Date(paper.deleted_at).toLocaleString()}`:"已删除"}</p></div></div><div><button onClick={()=>void restore(paper.id)}><ArchiveRestore/>恢复</button><button className="danger" onClick={()=>void destroy(paper)}><Trash2/>永久删除</button></div></article>)}</div>:<Empty icon={<Trash2/>} title="回收站是空的" text="被删除的论文会先来到这里。"/>}</div>
}

function PaperGrid({title,subtitle,papers,select}:{title:string;subtitle:string;papers:Paper[];select:Select}){return <div className="content-wrap"><header className="page-head"><p className="eyebrow">论文库</p><h1>{title}</h1><p>{subtitle}</p></header>{papers.length?<div className="paper-grid">{papers.map(paper=><PaperCard key={paper.id} paper={paper} onClick={()=>select({kind:"paper",id:paper.id})}/>)}</div>:<Empty icon={<Inbox/>} title="这里是空的" text="新的未分类论文会出现在这里。"/>}</div>}
function SearchView({select}:{select:Select}){const [query,setQuery]=useState("");const [results,setResults]=useState<SearchResult[]>([]);const submit=async(event:FormEvent)=>{event.preventDefault();setResults(await api.search(query))};return <div className="content-wrap"><header className="page-head"><p className="eyebrow">全文检索</p><h1>检索研究记忆</h1><p>搜索论文、项目、方法和 Codex 生成的结构化笔记。</p></header><form className="search-box" onSubmit={submit}><Search/><input autoFocus value={query} onChange={event=>setQuery(event.target.value)} placeholder="例如：注意力、基准、矛盾结论"/><button>搜索</button></form><div className="search-results">{results.map(result=><button key={`${result.entity_type}:${result.entity_id}`} onClick={()=>result.entity_type==="paper"&&select({kind:"paper",id:result.entity_id})}><span>{result.entity_type==="paper"?"论文":"项目"}</span><h3>{result.title}</h3><p dangerouslySetInnerHTML={{__html:result.snippet}}/></button>)}</div></div>}

function PaperCard({paper,onClick,remove}:{paper:Paper;onClick:()=>void;remove?:()=>void}){return <article className="paper-card"><button className="paper-card-main" onClick={onClick}><div className="paper-card-top"><FileText/><span>{paper.year??"—"}</span></div><h3>{paper.title}</h3><p>{paper.doi??paper.arxiv_id??paper.id}</p><div><span>{paper.note_path?"已整理":"处理中"}</span><ChevronRight/></div></button>{remove&&<button className="paper-remove" title="移出项目" onClick={remove}><X/></button>}</article>}
function SectionHead({title,action,onClick}:{title:string;action?:string;onClick?:()=>void}){return <div className="section-head"><h2>{title}</h2>{action&&<button onClick={onClick}>{action}<ChevronRight/></button>}</div>}
function Empty({icon,title,text}:{icon:ReactNode;title:string;text:string}){return <div className="empty-state"><div>{icon}</div><h3>{title}</h3><p>{text}</p></div>}

import type { FormEvent } from "react"
import { useCallback, useEffect, useLayoutEffect, useReducer, useRef, useState } from "react"
import { Activity, Archive, Blocks, Bot, History, MessageSquarePlus, Pencil, Sparkles, X } from "lucide-react"
import { ApiError, api, streamConversationEvents } from "./api"
import { CodexComposer, normalizeCodexSettings } from "./CodexComposer"
import { ConversationHistory, type ConversationHistoryView } from "./ConversationHistory"
import { CodexIntegrationsDrawer } from "./CodexIntegrationsDrawer"
import { CodexMessage } from "./CodexMessage"
import { CodexGoalBar } from "./CodexGoalBar"
import { conversationInitialState, conversationReducer } from "./conversation-store"
import { selectionsEqual, selectionForScopes, shouldClearConversationForSelection, type CodexSelection } from "./conversation-scope"
import { ConversationScrollController } from "./conversation-scroll"
import { latestAnswerCitations } from "./citation-overlay"
import { PanelCollapseButton } from "./PanelControls"
import type { Activity as TaskActivity, CandidateCitation, CodexCapabilities, CodexIntegrations, CodexRunSettings, CodexSkill, CodexToolPreference, Conversation, ConversationScope, MessageCitation, ResearchMode } from "./types"

export { ConversationProgress } from "./CodexMessage"

export interface CodexPanelProps {selection:CodexSelection;scopeLabel:string;activities:TaskActivity[];drawerOpen:boolean;onCollapse:()=>void;onCitation:(citation:MessageCitation)=>void;onCandidate?:(projectId:string,workId:string)=>void;onCitations:(citations:MessageCitation[])=>void;onSelect:(selection:CodexSelection)=>void;onResearchChanged?:(projectId:string)=>void;codexCapabilities?:CodexCapabilities}

const fallbackCapabilities:CodexCapabilities={default:{model:"gpt-5.6-luna",reasoning_effort:"medium",service_tier:null},models:[{id:"gpt-5.6-luna",display_name:"GPT-5.6-Luna",default_reasoning_effort:"medium",supported_reasoning_efforts:["low","medium","high","xhigh"],supports_fast:false}],supports_dynamic_tools:false}

function scopeFor(selection:CodexPanelProps["selection"]):ConversationScope[]{
  const projectId=selection.kind==="project"?selection.id:selection.projectId
  if(!projectId)return []
  const scopes:ConversationScope[]=[{scope_type:"project",scope_id:projectId}]
  if(selection.kind==="paper"&&selection.id)scopes.push({scope_type:"paper",scope_id:selection.id})
  return scopes
}

function conversationStorageKey(selection: CodexSelection): string {
  const projectId=selection.kind==="project"?selection.id:selection.projectId
  const paperId=selection.kind==="paper"?selection.id:undefined
  return `paper-codex.active-conversation.project:${projectId??"none"}:paper:${paperId??"none"}`
}

const candidateEvidenceLabel=(value:CandidateCitation["evidence_level"])=>({metadata:"仅元数据",abstract:"已核验摘要",fulltext:"已核验全文"})[value]

export const candidateProjectId=(citation:CandidateCitation)=>citation.project_id??null

export function CandidateCitationList({citations,onCandidate}:{citations:CandidateCitation[];onCandidate:(projectId:string,workId:string)=>void}){
  return <div className="candidate-citation-list" aria-label="外部候选论文证据">{citations.map(citation=><button key={citation.id} aria-label={`候选论文：${citation.title}`} onClick={()=>{const projectId=candidateProjectId(citation);if(projectId)onCandidate(projectId,citation.work_id)}}>
    <div><strong>{candidateEvidenceLabel(citation.evidence_level)}</strong><span>候选论文</span></div>
    <h4>{citation.title}</h4>
    <blockquote>{citation.quote}</blockquote>
    <p>{citation.explanation}</p>
  </button>)}</div>
}

export function CodexPanel({selection,scopeLabel,activities,drawerOpen,onCollapse,onCitation,onCandidate=()=>{},onCitations,onSelect,onResearchChanged,codexCapabilities:providedCapabilities}:CodexPanelProps){
  const [state,dispatch]=useReducer(conversationReducer,conversationInitialState)
  const [text,setText]=useState("");const [busy,setBusy]=useState(false);const [error,setError]=useState("")
  const [notice,setNotice]=useState("")
  const [researchMode,setResearchMode]=useState<ResearchMode>("auto")
  const [capabilities,setCapabilities]=useState<CodexCapabilities|null>(providedCapabilities??null)
  const [draftSettings,setDraftSettings]=useState<CodexRunSettings|null>(providedCapabilities?.default??null)
  const [integrations,setIntegrations]=useState<CodexIntegrations|null>(null)
  const [integrationsOpen,setIntegrationsOpen]=useState(false)
  const [integrationsLoading,setIntegrationsLoading]=useState(false)
  const [selectedSkill,setSelectedSkill]=useState<CodexSkill|null>(null)
  const [selectedTools,setSelectedTools]=useState<CodexToolPreference[]>([])
  const [historyView,setHistoryView]=useState<ConversationHistoryView>("active")
  const [archivedConversations,setArchivedConversations]=useState<Conversation[]>([])
  const [historyBusyId,setHistoryBusyId]=useState<string|null>(null)
  const preserveConversationForCitation=useRef(false)
  const historySwitchRequest=useRef(0)
  const feedRef=useRef<HTMLDivElement|null>(null)
  const feedContentRef=useRef<HTMLDivElement|null>(null)
  const scrollControllerRef=useRef(new ConversationScrollController(()=>feedRef.current))
  const scrollConversationRef=useRef<string|null>(null)
  const positionedConversationRef=useRef<string|null>(null)
  const scopeKey=conversationStorageKey(selection)
  const scopeKeyRef=useRef(scopeKey)
  scopeKeyRef.current=scopeKey
  const effectiveCapabilities=capabilities??fallbackCapabilities
  const projectResearchScope=Boolean(selection.kind==="project"?selection.id:selection.projectId)
  const projectContextId=selection.kind==="project"?selection.id:selection.projectId
  const controlledResearchAvailable=projectResearchScope&&effectiveCapabilities.supports_dynamic_tools
  const effectiveSettings=normalizeCodexSettings(effectiveCapabilities,state.activeSettings??draftSettings)
  const rememberConversation=(id:string)=>{try{localStorage.setItem(scopeKey,id)}catch{}}
  const refreshList=useCallback(async()=>{
    const requestedScopeKey=scopeKey
    const items=await api.conversations();dispatch({type:"conversations",items})
    if(scopeKeyRef.current!==requestedScopeKey)return
    try { const stored=localStorage.getItem(requestedScopeKey);if(stored&&items.some(item=>item.id===stored))dispatch({type:"active",id:stored}) } catch {}
  },[scopeKey])
  const refreshArchived=useCallback(async()=>setArchivedConversations(await api.conversations(true)),[])
  const loadIntegrations=useCallback(async(refresh=false)=>{
    setIntegrationsLoading(true)
    try{
      const next=await api.codexIntegrations(refresh)
      setIntegrations(next)
      setSelectedSkill(current=>{
        if(!current)return null
        return next.skills.find(skill=>skill.enabled&&skill.name===current.name&&skill.path===current.path)??null
      })
    }
    catch(value){setError(value instanceof Error?value.message:"加载 Codex 能力失败")}
    finally{setIntegrationsLoading(false)}
  },[])
  const loadDetail=useCallback(async(id:string)=>dispatch({type:"hydrate-detail",expectedConversationId:id,detail:await api.conversation(id)}),[])
  const requestIntegrations=()=>{if(!integrations&&!integrationsLoading)void loadIntegrations(false)}
  useEffect(()=>{void refreshList().catch(value=>setError(value instanceof Error?value.message:"加载对话失败"))},[refreshList])
  useEffect(()=>{if(providedCapabilities){setCapabilities(providedCapabilities);return}void api.codexCapabilities().then(setCapabilities).catch(value=>setError(value instanceof Error?value.message:"加载 Codex 能力失败"))},[providedCapabilities])
  useEffect(()=>{if(state.activeSettings)setDraftSettings(state.activeSettings)},[state.activeSettings])
  useEffect(()=>setResearchMode("auto"),[selection.kind,selection.id])
  useEffect(()=>{setSelectedSkill(null);setSelectedTools([]);setIntegrations(null);if(integrationsOpen)void loadIntegrations(true)},[projectContextId])
  useEffect(()=>{if(state.activeConversationId&&!state.scopes.length)void loadDetail(state.activeConversationId)},[state.activeConversationId,loadDetail])
  useEffect(()=>{if(!state.activeConversationId)return;const id=state.activeConversationId;void api.conversationGoal(id).then(goal=>dispatch({type:"goal-loaded",conversationId:id,goal})).catch(value=>setError(value instanceof Error?value.message:"加载目标失败"))},[state.activeConversationId])
  useLayoutEffect(()=>{
    const conversationId=state.activeConversationId
    if(scrollConversationRef.current!==conversationId){
      scrollConversationRef.current=conversationId
      positionedConversationRef.current=null
      scrollControllerRef.current.reset()
    }
    if(conversationId&&state.messageOrder.length&&positionedConversationRef.current!==conversationId){
      scrollControllerRef.current.positionInitial()
      positionedConversationRef.current=conversationId
    }
  },[state.activeConversationId,state.messageOrder.length])
  useEffect(()=>{
    const content=feedContentRef.current
    if(!content||typeof ResizeObserver==="undefined")return
    const observer=new ResizeObserver(()=>scrollControllerRef.current.followContent())
    observer.observe(content)
    return()=>observer.disconnect()
  },[state.activeConversationId])
  useEffect(()=>{
    if(typeof ResizeObserver==="undefined")scrollControllerRef.current.followContent()
  },[state.messages,state.messageOrder])
  useEffect(()=>{
    const pendingTarget=state.pendingSwitch?state.pendingSwitch.targetSelection:undefined
    if(state.pendingSwitch?.status==="resolved"&&pendingTarget&&selectionsEqual(pendingTarget,selection)){
      dispatch({type:"switch-complete",requestId:state.pendingSwitch.requestId})
      return
    }
    if(state.activeConversationId&&shouldClearConversationForSelection(state.scopes,selection,pendingTarget)){
      if(preserveConversationForCitation.current){preserveConversationForCitation.current=false;return}
      dispatch({type:"active",id:null})
    }
  },[selection.kind,selection.id,selection.projectId,state.activeConversationId,state.scopes,state.pendingSwitch])
  useEffect(()=>onCitations(latestAnswerCitations(state.messages,state.messageOrder)),[onCitations,state.messages,state.messageOrder])
  useEffect(()=>{if(!state.activeConversationId)return;const conversationId=state.activeConversationId;const controller=new AbortController();void streamConversationEvents(conversationId,state.lastEventId,event=>{dispatch({type:"event",event});if(event.type==="project-research-changed"){const projectId=event.payload.project_id;if(typeof projectId==="string")onResearchChanged?.(projectId)}if(["answer-completed","answer-failed","answer-cancelled"].includes(event.type)){void loadDetail(conversationId);if(event.type==="answer-completed")void refreshList()}},controller.signal).catch(()=>{});return()=>controller.abort()},[state.activeConversationId,loadDetail,onResearchChanged,refreshList])
  const create=async()=>{const scopes=scopeFor(selection);if(!scopes.length)throw new Error("请先创建或进入一个研究项目");const item=await api.createConversation("新对话",scopes,normalizeCodexSettings(effectiveCapabilities,null));rememberConversation(item.id);await refreshList();dispatch({type:"active",id:item.id});return item.id}
  const openConversation=async(id:string)=>{
    const requestId=++historySwitchRequest.current
    dispatch({type:"switch-start",requestId,conversationId:id})
    setError("")
    try{
      const detail=await api.conversation(id)
      if(historySwitchRequest.current!==requestId)return
      const target=selectionForScopes(detail.scopes)
      if(!target)throw new Error("该对话缺少有效的项目或论文作用域")
      dispatch({type:"switch-resolved",requestId,detail,targetSelection:target})
      try{localStorage.setItem(conversationStorageKey(target),id)}catch{}
      onSelect(target)
    }catch(value){
      if(historySwitchRequest.current!==requestId)return
      dispatch({type:"switch-failed",requestId})
      setError(value instanceof Error?value.message:"加载对话失败")
    }
  }
  const submit=async(event:FormEvent)=>{event.preventDefault();const content=text.trim();if(!content)return;setBusy(true);setError("");setNotice("");try{const compactCommand=content==="/compact";if(compactCommand){if(!state.activeConversationId)throw new Error("当前还没有可压缩的 Codex 对话");await api.compactConversation(state.activeConversationId);setText("");setNotice("当前对话上下文已由 Codex 压缩");return}const id=state.activeConversationId??await create();const goalMatch=content.match(/^\/goal(?:\s+(.+))?$/s);const prompt=goalMatch?goalMatch[1]?.trim()??"":content;if(goalMatch){if(!prompt)throw new Error("请在 /goal 后写明目标");const goal=await api.setConversationGoal(id,{objective:prompt,status:"active"});dispatch({type:"goal-loaded",conversationId:id,goal})}await api.sendConversationMessage(id,prompt,researchMode,selectedSkill?{name:selectedSkill.name,path:selectedSkill.path}:null,selectedTools);setText("");setSelectedSkill(null);setSelectedTools([]);setResearchMode("auto");await loadDetail(id)}catch(value){if(value instanceof ApiError&&value.status===409)void loadIntegrations(true);setError(value instanceof Error?value.message:"发送失败")}finally{setBusy(false)}}
  const updateGoal=async(value:{objective?:string;status?:"active"|"paused";token_budget?:number})=>{if(!state.activeConversationId)return;try{const goal=await api.setConversationGoal(state.activeConversationId,value);dispatch({type:"goal-loaded",conversationId:state.activeConversationId,goal})}catch(value){setError(value instanceof Error?value.message:"更新目标失败")}}
  const editGoal=()=>{if(!state.goal)return;const objective=window.prompt("编辑目标",state.goal.objective)?.trim();if(objective&&objective!==state.goal.objective)void updateGoal({objective})}
  const clearGoal=async()=>{if(!state.activeConversationId)return;try{await api.clearConversationGoal(state.activeConversationId);dispatch({type:"goal-loaded",conversationId:state.activeConversationId,goal:null})}catch(value){setError(value instanceof Error?value.message:"清除目标失败")}}
  const toggleTool=(preference:CodexToolPreference)=>setSelectedTools(current=>current.some(item=>item.server===preference.server&&item.tool===preference.tool)?current.filter(item=>item.server!==preference.server||item.tool!==preference.tool):[...current,preference])
  const openIntegrations=()=>{dispatch({type:"drawer",open:false});setIntegrationsOpen(true);if(!integrations)void loadIntegrations(false)}
  const rename=async()=>{if(!state.activeConversationId)return;const current=state.conversations.find(item=>item.id===state.activeConversationId);const title=window.prompt("对话名称",current?.title??"")?.trim();if(title){await api.updateConversation(state.activeConversationId,{title});await refreshList()}}
  const selectHistoryView=(view:ConversationHistoryView)=>{setHistoryView(view);if(view==="archived")void refreshArchived().catch(value=>setError(value instanceof Error?value.message:"加载已归档对话失败"))}
  const archiveConversation=async(id:string,keepDrawer:boolean)=>{setHistoryBusyId(id);setError("");try{await api.updateConversation(id,{archived:true});if(id===state.activeConversationId){try{localStorage.removeItem(scopeKey)}catch{};dispatch({type:"active",id:null})}await Promise.all([refreshList(),refreshArchived()]);if(keepDrawer)dispatch({type:"drawer",open:true,view:"history"})}catch(value){setError(value instanceof Error?value.message:"归档对话失败")}finally{setHistoryBusyId(null)}}
  const restoreConversation=async(id:string)=>{setHistoryBusyId(id);setError("");try{await api.updateConversation(id,{archived:false});await Promise.all([refreshList(),refreshArchived()])}catch(value){setError(value instanceof Error?value.message:"恢复对话失败")}finally{setHistoryBusyId(null)}}
  const deleteConversation=async(id:string)=>{const item=archivedConversations.find(conversation=>conversation.id===id);if(!item||!window.confirm(`永久删除“${item.title}”？此操作无法撤销。`))return;setHistoryBusyId(id);setError("");try{await api.deleteConversation(id);await refreshArchived()}catch(value){setError(value instanceof Error?value.message:"删除对话失败")}finally{setHistoryBusyId(null)}}
  const updateSettings=async(next:CodexRunSettings)=>{setDraftSettings(next);if(!state.activeConversationId)return;try{await api.updateConversation(state.activeConversationId,{settings:next});await loadDetail(state.activeConversationId);await refreshList()}catch(value){setError(value instanceof Error?value.message:"保存运行设置失败")}}
  const active=state.conversations.find(item=>item.id===state.activeConversationId)
  const answerRunning=state.messageOrder.some(id=>{const message=state.messages[id];return message.role==="assistant"&&["queued","running","streaming"].includes(message.status)})
  const suggestions=selection.kind==="paper"
    ?["概括这篇论文的核心贡献","这篇论文的实验设计可靠吗？","解释作者选择这个方法的动机"]
    :selection.kind==="project"
      ?["比较项目中论文的方法差异","这个方向还缺少哪些证据？","检索与当前选题相关的论文"]
      :["总结论文库中的主要研究线索","哪些论文之间存在方法联系？","找出值得继续研究的问题"]
  const placeholder=researchMode==="explicit"
    ?"描述你希望检索的研究问题…"
    :selection.kind==="paper"
      ?"询问这篇论文…"
      :selection.kind==="project"
        ?"询问这个项目…"
        :"询问整个论文库…"
  return <aside className={`activity-pane codex-pane workspace-panel${drawerOpen?" drawer-open":""}`} data-panel="codex">
    <header className="codex-task-header">
      <div className="codex-task-identity">
        <span className="codex-task-mark"><Sparkles/></span>
        <div>
          <span className="codex-task-label">Codex</span>
          <strong>{active?.title??"新对话"}</strong>
          <div className="codex-scope-pill" data-testid="codex-scope"><span>当前作用域</span><b>{scopeLabel}</b></div>
        </div>
      </div>
      <div className="codex-actions">
        <button className="codex-new-task" aria-label="新建对话" title="新建对话" onClick={()=>void create()}><MessageSquarePlus/><span>新对话</span></button>
        <button aria-label="Codex 能力" title="Codex 能力" onClick={openIntegrations}><Blocks/></button>
        <button aria-label="对话历史" title="对话历史" onClick={()=>{setIntegrationsOpen(false);setHistoryView("active");dispatch({type:"drawer",open:true,view:"history"});void refreshArchived().catch(value=>setError(value instanceof Error?value.message:"加载已归档对话失败"))}}><History/></button>
        <button aria-label="活动记录" title="活动记录" onClick={()=>{setIntegrationsOpen(false);dispatch({type:"drawer",open:true,view:"activity"})}}><Activity/></button>
        <button aria-label="重命名对话" title="重命名对话" onClick={()=>void rename()}><Pencil/></button>
        <button aria-label="归档对话" title="归档对话" disabled={!state.activeConversationId||answerRunning} onClick={()=>{if(state.activeConversationId)void archiveConversation(state.activeConversationId,false)}}><Archive/></button>
        <PanelCollapseButton label="Codex" direction="right" onCollapse={onCollapse}/>
      </div>
    </header>
    {state.goal&&<CodexGoalBar goal={state.goal} onPause={()=>void updateGoal({status:"paused"})} onResume={()=>void updateGoal({status:"active"})} onEdit={editGoal} onClear={()=>void clearGoal()}/>}
    <div className="conversation-feed" ref={feedRef} onScroll={()=>scrollControllerRef.current.handleScroll()}><div className="conversation-feed-content" ref={feedContentRef}>{state.messageOrder.length?state.messageOrder.map(id=>{const message=state.messages[id];return <div className="codex-message-group" key={id}><CodexMessage message={message} onCitation={citation=>{preserveConversationForCitation.current=true;onCitation(citation)}}/>{(message.candidate_citations?.length??0)>0&&<CandidateCitationList citations={message.candidate_citations} onCandidate={onCandidate}/>}</div>}):<div className="codex-empty-state"><span className="codex-empty-mark"><Bot/></span><h3>和 Codex 一起研究</h3><p>围绕当前内容提问、追踪证据，或继续扩展你的研究线索。</p><div className="codex-empty-prompts"><span>可以这样开始</span>{suggestions.map(suggestion=><button type="button" key={suggestion} onClick={()=>setText(suggestion)}>{suggestion}</button>)}</div></div>}<span data-testid="conversation-bottom" aria-hidden="true"/></div></div>
    {notice&&<p className="codex-notice">{notice}</p>}
    {error&&<p className="codex-error">{error}</p>}
    <CodexComposer text={text} placeholder={placeholder} busy={busy} answerRunning={answerRunning&&Boolean(state.activeConversationId)} projectResearchScope={projectResearchScope} controlledResearchAvailable={controlledResearchAvailable} researchMode={researchMode} capabilities={effectiveCapabilities} integrations={integrations} integrationsLoading={integrationsLoading} settings={effectiveSettings} selectedSkill={selectedSkill} selectedTools={selectedTools} onSelectSkill={setSelectedSkill} onClearSkill={()=>setSelectedSkill(null)} onToggleTool={toggleTool} onRequestIntegrations={requestIntegrations} onText={setText} onSubmit={submit} onCancel={()=>{if(state.activeConversationId)void api.cancelConversation(state.activeConversationId)}} onResearchMode={setResearchMode} onSettings={next=>void updateSettings(next)}/>
    <CodexIntegrationsDrawer open={integrationsOpen} integrations={integrations} loading={integrationsLoading} selectedSkill={selectedSkill} onClose={()=>setIntegrationsOpen(false)} onRefresh={()=>void loadIntegrations(true)} onSelectSkill={setSelectedSkill}/>
    {state.drawerOpen&&<div className="conversation-drawer"><header><strong>{state.drawerView==="history"?"对话历史":"活动记录"}</strong><button aria-label="关闭抽屉" onClick={()=>dispatch({type:"drawer",open:false})}><X/></button></header>{state.drawerView==="history"?<ConversationHistory view={historyView} active={state.conversations} archived={archivedConversations} activeConversationId={state.activeConversationId} busyId={historyBusyId} onView={selectHistoryView} onOpen={id=>void openConversation(id)} onArchive={id=>void archiveConversation(id,true)} onRestore={id=>void restoreConversation(id)} onDelete={id=>void deleteConversation(id)}/>:<div className="activity-feed">{activities.map(item=><div className="activity-item" key={item.id}><Activity/><div><p>{item.label}</p><span>{item.createdAt?new Date(item.createdAt).toLocaleTimeString():"刚刚"}</span></div></div>)}</div>}</div>}
  </aside>
}

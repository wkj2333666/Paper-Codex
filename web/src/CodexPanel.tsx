import { FormEvent, useCallback, useEffect, useReducer, useRef, useState } from "react"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"
import { Activity, Archive, Bot, History, LoaderCircle, MessageSquarePlus, Pencil, Send, SlidersHorizontal, Square, X } from "lucide-react"
import { api, streamConversationEvents } from "./api"
import { conversationInitialState, conversationReducer } from "./conversation-store"
import { scopesMatchSelection, selectionForScopes, type CodexSelection } from "./conversation-scope"
import { latestAnswerCitations } from "./citation-overlay"
import { PanelCollapseButton } from "./PanelControls"
import type { Activity as TaskActivity, ChatMessage, CodexCapabilities, CodexRunSettings, ConversationScope, MessageCitation } from "./types"

export interface CodexPanelProps {selection:CodexSelection;scopeLabel:string;activities:TaskActivity[];drawerOpen:boolean;onCollapse:()=>void;onCitation:(citation:MessageCitation)=>void;onCitations:(citations:MessageCitation[])=>void;onSelect:(selection:CodexSelection)=>void;codexCapabilities?:CodexCapabilities}

const fallbackCapabilities:CodexCapabilities={default:{model:"gpt-5.6-luna",reasoning_effort:"medium",service_tier:null},models:[{id:"gpt-5.6-luna",display_name:"GPT-5.6-Luna",default_reasoning_effort:"medium",supported_reasoning_efforts:["low","medium","high","xhigh"],supports_fast:false}]}

function normalizeSettings(capabilities:CodexCapabilities,settings?:CodexRunSettings|null):CodexRunSettings{
  const model=capabilities.models.find(item=>item.id===settings?.model)??capabilities.models[0]
  if(!model)return capabilities.default
  const reasoning_effort=model.supported_reasoning_efforts.includes(settings?.reasoning_effort??"")?settings!.reasoning_effort:model.default_reasoning_effort
  return {model:model.id,reasoning_effort,service_tier:settings?.service_tier==="priority"&&model.supports_fast?"priority":null}
}

function scopeFor(selection:CodexPanelProps["selection"]):ConversationScope[]{
  if(selection.kind==="paper"&&selection.id)return [{scope_type:"paper",scope_id:selection.id}]
  if(selection.kind==="project"&&selection.id)return [{scope_type:"project",scope_id:selection.id}]
  return [{scope_type:"global",scope_id:null}]
}

function conversationStorageKey(selection: CodexSelection): string {
  return `paper-codex.active-conversation.${selection.kind}:${selection.id ?? "all"}`
}

export function ConversationProgress({phase}:{phase:ChatMessage["progress_phase"]}){
  return <div className="conversation-progress" role="status"><LoaderCircle className="spin"/><span>{phase==="reading"?"Codex 正在读取论文…":"Codex 正在分析证据并组织回答…"}</span></div>
}

export function CodexPanel({selection,scopeLabel,activities,drawerOpen,onCollapse,onCitation,onCitations,onSelect,codexCapabilities:providedCapabilities}:CodexPanelProps){
  const [state,dispatch]=useReducer(conversationReducer,conversationInitialState)
  const [text,setText]=useState("");const [busy,setBusy]=useState(false);const [error,setError]=useState("")
  const [capabilities,setCapabilities]=useState<CodexCapabilities|null>(providedCapabilities??null)
  const [draftSettings,setDraftSettings]=useState<CodexRunSettings|null>(providedCapabilities?.default??null)
  const preserveConversationForCitation=useRef(false)
  const scopeKey=conversationStorageKey(selection)
  const effectiveCapabilities=capabilities??fallbackCapabilities
  const effectiveSettings=normalizeSettings(effectiveCapabilities,state.activeSettings??draftSettings)
  const rememberConversation=(id:string)=>{try{localStorage.setItem(scopeKey,id)}catch{}}
  const refreshList=useCallback(async()=>{
    const items=await api.conversations();dispatch({type:"conversations",items})
    try { const stored=localStorage.getItem(scopeKey);if(stored&&items.some(item=>item.id===stored))dispatch({type:"active",id:stored}) } catch {}
  },[scopeKey])
  const loadDetail=useCallback(async(id:string)=>dispatch({type:"detail",detail:await api.conversation(id)}),[])
  useEffect(()=>{void refreshList().catch(value=>setError(value instanceof Error?value.message:"加载对话失败"))},[refreshList])
  useEffect(()=>{if(providedCapabilities){setCapabilities(providedCapabilities);return}void api.codexCapabilities().then(setCapabilities).catch(value=>setError(value instanceof Error?value.message:"加载 Codex 能力失败"))},[providedCapabilities])
  useEffect(()=>{if(state.activeSettings)setDraftSettings(state.activeSettings)},[state.activeSettings])
  useEffect(()=>{if(state.activeConversationId)void loadDetail(state.activeConversationId)},[state.activeConversationId,loadDetail])
  useEffect(()=>{if(state.activeConversationId&&state.scopes.length&&!scopesMatchSelection(state.scopes,selection)){if(preserveConversationForCitation.current){preserveConversationForCitation.current=false;return}dispatch({type:"active",id:null})}},[selection.kind,selection.id,state.activeConversationId,state.scopes])
  useEffect(()=>onCitations(latestAnswerCitations(state.messages,state.messageOrder)),[onCitations,state.messages,state.messageOrder])
  useEffect(()=>{if(!state.activeConversationId)return;const conversationId=state.activeConversationId;const controller=new AbortController();void streamConversationEvents(conversationId,state.lastEventId,event=>{dispatch({type:"event",event});if(["answer-completed","answer-failed","answer-cancelled"].includes(event.type)){void loadDetail(conversationId);if(event.type==="answer-completed")void refreshList()}},controller.signal).catch(()=>{});return()=>controller.abort()},[state.activeConversationId,loadDetail,refreshList])
  const create=async()=>{const item=await api.createConversation("新对话",scopeFor(selection),normalizeSettings(effectiveCapabilities,null));rememberConversation(item.id);await refreshList();dispatch({type:"active",id:item.id});return item.id}
  const openConversation=async(id:string)=>{const detail=await api.conversation(id);const target=selectionForScopes(detail.scopes);if(target){try{localStorage.setItem(conversationStorageKey(target),id)}catch{};onSelect(target)}dispatch({type:"detail",detail})}
  const submit=async(event:FormEvent)=>{event.preventDefault();const content=text.trim();if(!content)return;setBusy(true);setError("");try{const id=state.activeConversationId??await create();await api.sendConversationMessage(id,content);setText("");await loadDetail(id)}catch(value){setError(value instanceof Error?value.message:"发送失败")}finally{setBusy(false)}}
  const rename=async()=>{if(!state.activeConversationId)return;const current=state.conversations.find(item=>item.id===state.activeConversationId);const title=window.prompt("对话名称",current?.title??"")?.trim();if(title){await api.updateConversation(state.activeConversationId,{title});await refreshList()}}
  const archive=async()=>{if(!state.activeConversationId)return;await api.updateConversation(state.activeConversationId,{archived:true});try{localStorage.removeItem(scopeKey)}catch{};dispatch({type:"active",id:null});await refreshList()}
  const updateSettings=async(next:CodexRunSettings)=>{setDraftSettings(next);if(!state.activeConversationId)return;try{await api.updateConversation(state.activeConversationId,{settings:next});await loadDetail(state.activeConversationId);await refreshList()}catch(value){setError(value instanceof Error?value.message:"保存运行设置失败")}}
  const selectedModel=effectiveCapabilities.models.find(item=>item.id===effectiveSettings.model)??effectiveCapabilities.models[0]
  const active=state.conversations.find(item=>item.id===state.activeConversationId)
  const answerRunning=state.messageOrder.some(id=>{const message=state.messages[id];return message.role==="assistant"&&["queued","running","streaming"].includes(message.status)})
  return <aside className={`activity-pane codex-pane workspace-panel${drawerOpen?" drawer-open":""}`} data-panel="codex">
    <header className="codex-header"><div><Bot/><strong>{active?.title??"Codex 对话"}</strong></div><div className="codex-actions"><button aria-label="新建对话" title="新建对话" onClick={()=>void create()}><MessageSquarePlus/></button><details className="codex-settings-popover"><summary aria-label="Codex 运行设置" title="Codex 运行设置"><SlidersHorizontal/></summary><div className="codex-settings" role="dialog" aria-label="Codex 运行设置"><label>模型<select value={effectiveSettings.model} onChange={event=>{const model=effectiveCapabilities.models.find(item=>item.id===event.target.value)??selectedModel;if(model)void updateSettings(normalizeSettings(effectiveCapabilities,{...effectiveSettings,model:model.id,reasoning_effort:model.default_reasoning_effort,service_tier:null}))}}>{effectiveCapabilities.models.map(model=><option key={model.id} value={model.id}>{model.display_name}</option>)}</select></label><label>推理强度<select value={effectiveSettings.reasoning_effort} onChange={event=>void updateSettings({...effectiveSettings,reasoning_effort:event.target.value})}>{(selectedModel?.supported_reasoning_efforts??[]).map(effort=><option key={effort} value={effort}>{effort}</option>)}</select></label><label>速度<select value={effectiveSettings.service_tier==="priority"?"fast":"standard"} disabled={!selectedModel?.supports_fast} onChange={event=>void updateSettings({...effectiveSettings,service_tier:event.target.value==="fast"?"priority":null})}><option value="standard">标准</option><option value="fast">快速</option></select></label></div></details><button aria-label="重命名对话" onClick={()=>void rename()}><Pencil/></button><button aria-label="归档对话" onClick={()=>void archive()}><Archive/></button><PanelCollapseButton label="Codex" direction="right" onCollapse={onCollapse}/></div></header>
    <div className="codex-scope-banner" data-testid="codex-scope"><span>当前作用域</span><strong>{scopeLabel}</strong></div>
    <nav className="codex-subnav"><button onClick={()=>dispatch({type:"drawer",open:true,view:"history"})}><History/>对话历史</button><button onClick={()=>dispatch({type:"drawer",open:true,view:"activity"})}><Activity/>活动记录</button></nav>
    <div className="conversation-feed">{state.messageOrder.length?state.messageOrder.map(id=>{const message=state.messages[id];return <article key={id} className={`chat-message ${message.role}`}><div className="message-body">{message.role==="assistant"&&!message.content&&["queued","running","streaming"].includes(message.status)?<ConversationProgress phase={message.progress_phase}/>:<ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown>}{message.status==="failed"&&<p className="message-error">{message.error}</p>}</div>{message.citations.length>0&&<div className="citation-list">{message.citations.map(citation=><button key={citation.id} onClick={()=>{preserveConversationForCitation.current=true;onCitation(citation)}}><strong>第 {citation.page} 页</strong><span>{citation.quote}</span></button>)}</div>}</article>}):<div className="quiet"><Bot/><p>新建对话，直接询问论文的方法、动机与实验设计。</p></div>}</div>
    {error&&<p className="codex-error">{error}</p>}
    <form className="conversation-composer" onSubmit={submit}><textarea value={text} onChange={event=>setText(event.target.value)} placeholder={selection.kind==="paper"?"询问这篇论文…":selection.kind==="project"?"询问这个项目…":"询问整个论文库…"}/>{answerRunning&&state.activeConversationId?<button type="button" aria-label="停止回答" onClick={()=>void api.cancelConversation(state.activeConversationId!)}><Square/></button>:<button aria-label="发送消息" disabled={busy||!text.trim()}>{busy?<LoaderCircle className="spin"/>:<Send/>}</button>}</form>
    {state.drawerOpen&&<div className="conversation-drawer"><header><strong>{state.drawerView==="history"?"对话历史":"活动记录"}</strong><button aria-label="关闭抽屉" onClick={()=>dispatch({type:"drawer",open:false})}><X/></button></header>{state.drawerView==="history"?<div className="conversation-list">{state.conversations.map(item=><button className={item.id===state.activeConversationId?"active":""} key={item.id} onClick={()=>void openConversation(item.id)}><strong>{item.title}</strong><span>{new Date(item.updated_at).toLocaleString()}</span></button>)}</div>:<div className="activity-feed">{activities.map(item=><div className="activity-item" key={item.id}><Activity/><div><p>{item.label}</p><span>{item.createdAt?new Date(item.createdAt).toLocaleTimeString():"刚刚"}</span></div></div>)}</div>}</div>}
  </aside>
}

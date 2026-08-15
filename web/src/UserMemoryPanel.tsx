import { useCallback, useEffect, useMemo, useState } from "react"
import { EyeOff, Pencil, Plus, Trash2 } from "lucide-react"
import { api } from "./api"
import { memoryConfidenceLabel, memoryKindLabels, memoryKindsForScope, memorySourceLabel, memoryUpdatedLabel } from "./user-memory"
import type { MemoryItem, MemoryKind } from "./types"

export function UserMemoryPanel({projectId,onError}:{projectId:string|null;onError:(message:string)=>void}){
  const [scope,setScope]=useState<"global"|"project">(projectId?"project":"global")
  const [items,setItems]=useState<MemoryItem[]>([])
  const [kind,setKind]=useState<MemoryKind>(projectId?"goal":"preference")
  const [value,setValue]=useState("")
  const [busy,setBusy]=useState(false)
  const availableKinds=useMemo(()=>memoryKindsForScope(scope),[scope])
  const load=useCallback(async()=>{setBusy(true);try{setItems(await api.memories(scope,scope==="project"?projectId??undefined:undefined))}catch(error){onError(error instanceof Error?error.message:"加载记忆失败")}finally{setBusy(false)}},[onError,projectId,scope])
  useEffect(()=>{if(scope==="project"&&!projectId)setScope("global");else void load()},[load,projectId,scope])
  useEffect(()=>{if(!availableKinds.includes(kind))setKind(availableKinds[0])},[availableKinds,kind])
  const chooseScope=(next:"global"|"project")=>{setScope(next);setKind(memoryKindsForScope(next)[0])}
  const create=async()=>{const next=value.trim();if(!next||!availableKinds.includes(kind))return;setBusy(true);try{await api.createMemory({scope_type:scope,scope_id:scope==="project"?projectId:null,kind,value:next});setValue("");await load()}catch(error){onError(error instanceof Error?error.message:"保存记忆失败")}finally{setBusy(false)}}
  const edit=async(item:MemoryItem)=>{const next=window.prompt("编辑记忆",item.value)?.trim();if(!next||next===item.value)return;try{await api.updateMemory(item.id,{value:next});await load()}catch(error){onError(error instanceof Error?error.message:"更新记忆失败")}}
  const dismiss=async(item:MemoryItem)=>{try{await api.updateMemory(item.id,{status:"dismissed"});await load()}catch(error){onError(error instanceof Error?error.message:"隐藏记忆失败")}}
  const remove=async(item:MemoryItem)=>{if(!window.confirm(`永久删除“${item.value}”？`))return;try{await api.deleteMemory(item.id);await load()}catch(error){onError(error instanceof Error?error.message:"删除记忆失败")}}
  return <div className="user-memory-panel">
    <div className="memory-scope-tabs" role="tablist" aria-label="记忆作用域">
      <button role="tab" aria-selected={scope==="global"} onClick={()=>chooseScope("global")}>全局画像</button>
      <button role="tab" aria-selected={scope==="project"} disabled={!projectId} onClick={()=>chooseScope("project")}>当前项目</button>
    </div>
    <div className="memory-create-row">
      <select aria-label="记忆类型" value={kind} onChange={event=>setKind(event.target.value as MemoryKind)}>{availableKinds.map(key=><option key={key} value={key}>{memoryKindLabels[key]}</option>)}</select>
      <input aria-label="新记忆" value={value} maxLength={2000} onChange={event=>setValue(event.target.value)} onKeyDown={event=>{if(event.key==="Enter")void create()}}/>
      <button aria-label="添加记忆" title="添加记忆" disabled={busy||!value.trim()} onClick={()=>void create()}><Plus/></button>
    </div>
    {busy&&!items.length?<p className="memory-empty">正在加载…</p>:items.length?<div className="memory-list">{items.map(item=><div className="memory-row" key={item.id}>
      <div><span>{memoryKindLabels[item.kind]}</span><p>{item.value}</p><small>{memorySourceLabel(item.source)} · {memoryConfidenceLabel(item.confidence)} · {memoryUpdatedLabel(item.updated_at)}</small></div>
      <div className="memory-row-actions"><button aria-label="编辑记忆" title="编辑记忆" onClick={()=>void edit(item)}><Pencil/></button><button aria-label="隐藏记忆" title="隐藏记忆" onClick={()=>void dismiss(item)}><EyeOff/></button><button aria-label="删除记忆" title="删除记忆" onClick={()=>void remove(item)}><Trash2/></button></div>
    </div>)}</div>:<p className="memory-empty">还没有记忆</p>}
  </div>
}

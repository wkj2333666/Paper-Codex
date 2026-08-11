import { useCallback, useEffect, useReducer, useRef, useState } from "react"
import { AlertTriangle, Check, Cloud, LoaderCircle, RefreshCcw, Save } from "lucide-react"
import { Crepe } from "@milkdown/crepe"
import { replaceAll } from "@milkdown/kit/utils"
import "@milkdown/crepe/theme/common/style.css"
import "@milkdown/crepe/theme/frame.css"
import { ApiError, api } from "./api"
import { initialProjectReadmeState, normalizeProjectMarkdown, projectReadmeReducer, type ProjectReadmeDraft } from "./project-readme-state"
import "./project-readme.css"

function replaceEditorMarkdown(editor:Crepe,markdown:string,replacing:{current:string|null}){
  const normalized=normalizeProjectMarkdown(markdown)
  if(normalizeProjectMarkdown(editor.getMarkdown())===normalized)return
  replacing.current=normalized
  editor.editor.action(replaceAll(markdown))
}

function MarkdownSurface({markdown,onChange,onFailure}:{markdown:string;onChange:(markdown:string)=>void;onFailure:(message:string)=>void}){
  const root=useRef<HTMLDivElement>(null)
  const editor=useRef<Crepe|null>(null)
  const desired=useRef(normalizeProjectMarkdown(markdown))
  const replacing=useRef<string|null>(null)
  const change=useRef(onChange)
  const failure=useRef(onFailure)
  desired.current=normalizeProjectMarkdown(markdown)
  change.current=onChange
  failure.current=onFailure
  useEffect(()=>{
    if(!root.current)return
    let active=true
    const crepe=new Crepe({
      root:root.current,
      defaultValue:desired.current,
      features:{
        [Crepe.Feature.AI]:false,
        [Crepe.Feature.ImageBlock]:false,
      },
    })
    crepe.on(listener=>listener.markdownUpdated((_ctx,value)=>{
      const next=normalizeProjectMarkdown(value)
      if(replacing.current!==null){
        const expected=replacing.current
        replacing.current=null
        if(next===expected)return
      }
      change.current(next)
    }))
    void crepe.create().then(()=>{
      if(!active)return
      editor.current=crepe
      replaceEditorMarkdown(crepe,desired.current,replacing)
    }).catch(error=>{if(active)failure.current(error instanceof Error?error.message:"编辑器初始化失败")})
    return()=>{active=false;if(editor.current===crepe)editor.current=null;void crepe.destroy()}
  },[])
  useEffect(()=>{if(editor.current)replaceEditorMarkdown(editor.current,markdown,replacing)},[markdown])
  return <div className="project-readme-crepe" ref={root}/>
}

export default function ProjectReadmeEditor({projectId}:{projectId:string}){
  const [state,dispatch]=useReducer(projectReadmeReducer,initialProjectReadmeState)
  const [reloadError,setReloadError]=useState("")
  const requestId=useRef(0)

  const load=useCallback(async()=>{
    dispatch({type:"loading"})
    try{
      const value=await api.projectReadme(projectId)
      dispatch({type:"loaded",value,draft:readDraft(projectId)})
    }
    catch(error){dispatch({type:"failed",requestId:0,error:error instanceof Error?error.message:"项目笔记加载失败"})}
  },[projectId])
  useEffect(()=>{void load()},[load])

  useEffect(()=>{
    if(state.status==="saved")clearDraft(projectId)
    else if(state.status==="dirty"&&state.markdown!==state.savedMarkdown)writeDraft(projectId,{markdown:state.markdown,baseRevision:state.revision})
  },[projectId,state.markdown,state.revision,state.savedMarkdown,state.status])

  const save=useCallback(async(markdown:string,revision:string)=>{
    const id=++requestId.current
    dispatch({type:"saving",requestId:id})
    try{
      const value=await api.saveProjectReadme(projectId,{markdown:normalizeProjectMarkdown(markdown),expected_revision:revision})
      dispatch({type:"saved",requestId:id,value})
    }catch(error){
      if(error instanceof ApiError&&error.status===409){
        dispatch({type:"conflict",requestId:id,currentRevision:typeof error.body.current_revision==="string"?error.body.current_revision:""})
      }else dispatch({type:"failed",requestId:id,error:error instanceof Error?error.message:"项目笔记保存失败"})
    }
  },[projectId])

  useEffect(()=>{
    if(state.status!=="dirty")return
    const timer=window.setTimeout(()=>void save(state.markdown,state.revision),700)
    return()=>window.clearTimeout(timer)
  },[save,state.markdown,state.revision,state.status])

  const overwrite=async()=>{
    const localMarkdown=state.markdown
    try{
      const latest=await api.projectReadme(projectId)
      writeDraft(projectId,{markdown:localMarkdown,baseRevision:latest.revision})
      await save(localMarkdown,latest.revision)
    }catch(error){
      const id=++requestId.current
      dispatch({type:"saving",requestId:id})
      dispatch({type:"failed",requestId:id,error:error instanceof Error?error.message:"无法读取服务器版本"})
    }
  }

  const reloadFromServer=async()=>{
    setReloadError("")
    try{
      const value=await api.projectReadme(projectId)
      clearDraft(projectId)
      dispatch({type:"loaded",value})
    }catch(error){setReloadError(error instanceof Error?error.message:"无法读取服务器版本")}
  }

  if(state.status==="loading")return <div className="project-readme-loading"><LoaderCircle className="spin"/>正在载入项目笔记…</div>
  return <section className="project-readme-workspace" aria-label="项目笔记编辑器">
    <header className="project-readme-header">
      <div><span>README.md</span><p>像文档一样直接写作，内容会保存为项目中的 Markdown。</p></div>
      <div className={`project-readme-status ${state.status}`} aria-live="polite">{statusIcon(state.status)}{statusText(state.status)}</div>
    </header>
    {state.status==="conflict"&&<div className="project-readme-conflict" role="alert"><AlertTriangle/><div><strong>服务器上有更新</strong><p>{reloadError||"请选择载入服务器版本，或明确以当前内容覆盖。"}</p></div><button onClick={()=>void reloadFromServer()}><RefreshCcw/>载入服务器版本</button><button className="danger" onClick={()=>void overwrite()}><Save/>以当前内容覆盖</button></div>}
    {state.status==="error"&&<div className="project-readme-conflict" role="alert"><AlertTriangle/><div><strong>笔记暂未保存</strong><p>{state.error}</p></div><button onClick={()=>void save(state.markdown,state.revision)}><RefreshCcw/>重试</button></div>}
    <MarkdownSurface markdown={state.markdown} onChange={markdown=>{const existing=readDraft(projectId);writeDraft(projectId,{markdown,baseRevision:state.status==="conflict"&&existing?existing.baseRevision:state.revision});dispatch({type:"edit",markdown})}} onFailure={message=>{const id=++requestId.current;dispatch({type:"saving",requestId:id});dispatch({type:"failed",requestId:id,error:message})}}/>
  </section>
}

const draftKey=(projectId:string)=>`paper-codex.project-readme-draft:${projectId}`

function readDraft(projectId:string):ProjectReadmeDraft|null{
  try{
    const parsed=JSON.parse(window.localStorage.getItem(draftKey(projectId))??"null") as Partial<ProjectReadmeDraft>|null
    return parsed&&typeof parsed.markdown==="string"&&typeof parsed.baseRevision==="string"?{markdown:parsed.markdown,baseRevision:parsed.baseRevision}:null
  }catch{return null}
}

function writeDraft(projectId:string,draft:ProjectReadmeDraft){
  try{window.localStorage.setItem(draftKey(projectId),JSON.stringify(draft))}catch{/* Browser storage is best-effort. */}
}

function clearDraft(projectId:string){
  try{window.localStorage.removeItem(draftKey(projectId))}catch{/* Browser storage is best-effort. */}
}

function statusText(status:string){
  return {saved:"已保存",dirty:"等待保存",saving:"正在保存",conflict:"存在冲突",error:"保存失败"}[status]??""
}

function statusIcon(status:string){
  if(status==="saving")return <LoaderCircle className="spin"/>
  if(status==="saved")return <Check/>
  if(status==="dirty")return <Cloud/>
  return <AlertTriangle/>
}

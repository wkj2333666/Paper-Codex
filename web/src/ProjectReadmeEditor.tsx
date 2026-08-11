import { useCallback, useEffect, useReducer, useRef } from "react"
import { AlertTriangle, Check, Cloud, LoaderCircle, RefreshCcw, Save } from "lucide-react"
import { Crepe } from "@milkdown/crepe"
import "@milkdown/crepe/theme/common/style.css"
import "@milkdown/crepe/theme/frame.css"
import { ApiError, api } from "./api"
import { initialProjectReadmeState, projectReadmeReducer } from "./project-readme-state"
import "./project-readme.css"

function MarkdownSurface({initialMarkdown,onChange,onFailure}:{initialMarkdown:string;onChange:(markdown:string)=>void;onFailure:(message:string)=>void}){
  const root=useRef<HTMLDivElement>(null)
  const initial=useRef(initialMarkdown)
  const change=useRef(onChange)
  const failure=useRef(onFailure)
  change.current=onChange
  failure.current=onFailure
  useEffect(()=>{
    if(!root.current)return
    const crepe=new Crepe({
      root:root.current,
      defaultValue:initial.current,
      features:{
        [Crepe.Feature.AI]:false,
        [Crepe.Feature.ImageBlock]:false,
      },
    })
    crepe.on(listener=>listener.markdownUpdated((_ctx,markdown)=>change.current(markdown)))
    void crepe.create().catch(error=>failure.current(error instanceof Error?error.message:"编辑器初始化失败"))
    return()=>{void crepe.destroy()}
  },[])
  return <div className="project-readme-crepe" ref={root}/>
}

export default function ProjectReadmeEditor({projectId}:{projectId:string}){
  const [state,dispatch]=useReducer(projectReadmeReducer,initialProjectReadmeState)
  const requestId=useRef(0)

  const load=useCallback(async()=>{
    dispatch({type:"loading"})
    try{dispatch({type:"loaded",value:await api.projectReadme(projectId)})}
    catch(error){dispatch({type:"failed",requestId:0,error:error instanceof Error?error.message:"项目笔记加载失败"})}
  },[projectId])
  useEffect(()=>{void load()},[load])

  const save=useCallback(async(markdown:string,revision:string)=>{
    const id=++requestId.current
    dispatch({type:"saving",requestId:id})
    try{
      const value=await api.saveProjectReadme(projectId,{markdown,expected_revision:revision})
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
      await save(localMarkdown,latest.revision)
    }catch(error){
      const id=++requestId.current
      dispatch({type:"saving",requestId:id})
      dispatch({type:"failed",requestId:id,error:error instanceof Error?error.message:"无法读取服务器版本"})
    }
  }

  if(state.status==="loading")return <div className="project-readme-loading"><LoaderCircle className="spin"/>正在载入项目笔记…</div>
  return <section className="project-readme-workspace" aria-label="项目笔记编辑器">
    <header className="project-readme-header">
      <div><span>README.md</span><p>像文档一样直接写作，内容会保存为项目中的 Markdown。</p></div>
      <div className={`project-readme-status ${state.status}`} aria-live="polite">{statusIcon(state.status)}{statusText(state.status)}</div>
    </header>
    {state.status==="conflict"&&<div className="project-readme-conflict" role="alert"><AlertTriangle/><div><strong>服务器上有更新</strong><p>请选择载入服务器版本，或明确以当前内容覆盖。</p></div><button onClick={()=>void load()}><RefreshCcw/>载入服务器版本</button><button className="danger" onClick={()=>void overwrite()}><Save/>以当前内容覆盖</button></div>}
    {state.status==="error"&&<div className="project-readme-conflict" role="alert"><AlertTriangle/><div><strong>笔记暂未保存</strong><p>{state.error}</p></div><button onClick={()=>void save(state.markdown,state.revision)}><RefreshCcw/>重试</button></div>}
    <MarkdownSurface initialMarkdown={state.markdown} onChange={markdown=>dispatch({type:"edit",markdown})} onFailure={message=>{const id=++requestId.current;dispatch({type:"saving",requestId:id});dispatch({type:"failed",requestId:id,error:message})}}/>
  </section>
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

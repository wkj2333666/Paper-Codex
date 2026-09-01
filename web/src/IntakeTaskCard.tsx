import { CircleAlert, CircleX, LoaderCircle, Square, X } from "lucide-react"
import { intakeStateLabel, taskSource } from "./intake-status"
import type { DownloadAttempt, Task, TaskFailureDetails } from "./types"

function failureDetails(task:Task):TaskFailureDetails|null{
  if(!task.error_details_json)return null
  try{
    const value=JSON.parse(task.error_details_json) as Partial<TaskFailureDetails>
    if(typeof value.code!=="string"||!Array.isArray(value.attempts))return null
    const attempts=value.attempts.filter((attempt):attempt is DownloadAttempt=>{
      if(!attempt||typeof attempt!=="object")return false
      return typeof attempt.provider==="string"&&typeof attempt.url==="string"&&typeof attempt.reason_code==="string"&&typeof attempt.reason==="string"&&(attempt.status===null||typeof attempt.status==="number")
    })
    return {code:value.code,attempts}
  }catch{return null}
}

function safeSource(url:string):{href:string;label:string}|null{
  try{const parsed=new URL(url);if(parsed.protocol!=="http:"&&parsed.protocol!=="https:")return null;return {href:`${parsed.origin}${parsed.pathname}`,label:`${parsed.hostname}${parsed.pathname}`}}catch{return null}
}

export function IntakeTaskCard({task,onCancel,onDismiss}:{task:Task;onCancel:(id:string)=>void;onDismiss:(id:string)=>void}){
  const terminal=task.state==="failed"||task.state==="cancelled"
  const details=failureDetails(task)
  const StatusIcon=task.state==="failed"?CircleAlert:task.state==="cancelled"?CircleX:LoaderCircle
  return <article className={`paper-card intake-task-card intake-task-${task.state}`} aria-label={`${taskSource(task)}：${intakeStateLabel(task.state)}`}>
    <div className="paper-card-top"><StatusIcon className={terminal?"":"spin"}/><span>{intakeStateLabel(task.state)}</span>{terminal?<button type="button" className="task-card-action" aria-label="关闭记录" title="关闭记录" onClick={()=>onDismiss(task.id)}><X/></button>:<button type="button" className="task-card-action" aria-label="取消任务" title="取消任务" onClick={()=>onCancel(task.id)}><Square/></button>}</div>
    <h3>{taskSource(task)}</h3>
    <p>{task.error||task.status_note||(terminal?"任务已取消":"Codex 正在后台处理这篇论文")}</p>
    {!!task.analysis_warnings?.length&&<details className="task-analysis-warnings">
      <summary>{task.analysis_warnings.length} 条图谱关系未写入</summary>
      <ul>{task.analysis_warnings.map((warning,index)=><li key={`${index}-${warning}`}>{warning}</li>)}</ul>
    </details>}
    {!!details?.attempts.length&&<details className="task-source-attempts">
      <summary>查看 {details.attempts.length} 个来源尝试</summary>
      <ul>{details.attempts.map((attempt,index)=>{const source=safeSource(attempt.url);return <li key={`${attempt.provider}-${index}`}><div><strong>{attempt.provider}</strong>{attempt.status!==null&&<span>HTTP {attempt.status}</span>}</div>{source?<a href={source.href} target="_blank" rel="noreferrer">{source.label}</a>:<span>来源地址不可显示</span>}<p>{attempt.reason}</p></li>})}</ul>
    </details>}
    <div><span>{terminal?"可关闭此记录":"后台处理中"}</span></div>
  </article>
}

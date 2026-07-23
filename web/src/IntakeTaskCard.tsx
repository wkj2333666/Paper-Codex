import { CircleAlert, CircleX, LoaderCircle, Square, X } from "lucide-react"
import { intakeStateLabel, taskSource } from "./intake-status"
import type { Task } from "./types"

export function IntakeTaskCard({task,onCancel,onDismiss}:{task:Task;onCancel:(id:string)=>void;onDismiss:(id:string)=>void}){
  const terminal=task.state==="failed"||task.state==="cancelled"
  const StatusIcon=task.state==="failed"?CircleAlert:task.state==="cancelled"?CircleX:LoaderCircle
  return <article className={`paper-card intake-task-card intake-task-${task.state}`} aria-label={`${taskSource(task)}：${intakeStateLabel(task.state)}`}>
    <div className="paper-card-top"><StatusIcon className={terminal?"":"spin"}/><span>{intakeStateLabel(task.state)}</span>{terminal?<button type="button" className="task-card-action" aria-label="关闭记录" title="关闭记录" onClick={()=>onDismiss(task.id)}><X/></button>:<button type="button" className="task-card-action" aria-label="取消任务" title="取消任务" onClick={()=>onCancel(task.id)}><Square/></button>}</div>
    <h3>{taskSource(task)}</h3>
    <p>{task.error||(terminal?"任务已取消":"Codex 正在后台处理这篇论文")}</p>
    <div><span>{terminal?"可关闭此记录":"后台处理中"}</span></div>
  </article>
}

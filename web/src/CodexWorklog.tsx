import { Check, ChevronDown, Circle, LoaderCircle, Search, SquareTerminal, Wrench } from "lucide-react"
import type { CodexWorkItem, CodexWorklog as Worklog } from "./types"

const ItemIcon=({item}:{item:CodexWorkItem})=>item.status==="completed"?<Check/>:item.item_type==="webSearch"?<Search/>:item.item_type==="commandExecution"?<SquareTerminal/>:<Wrench/>

export function CodexWorklog({worklog,active}:{worklog:Worklog;active:boolean}){
  const summaries=worklog.summaries.filter(item=>item.text.trim())
  const items=Object.values(worklog.items)
  if(!summaries.length&&!worklog.plan?.steps.length&&!items.length)return null
  return <details className="codex-live-work" open={active}>
    <summary><span>{active?<LoaderCircle className="spin"/>:<Check/>}<strong>{active?"Codex 正在工作":"查看工作过程"}</strong></span><ChevronDown/></summary>
    <div className="codex-live-work-body">
      {summaries.length>0&&<div className="codex-reasoning-summaries">{summaries.map(item=><p key={`${item.item_id}:${item.summary_index}`}>{item.text}</p>)}</div>}
      {worklog.plan&&<div className="codex-plan">{worklog.plan.explanation&&<p>{worklog.plan.explanation}</p>}<ol>{worklog.plan.steps.map((step,index)=><li className={`plan-${step.status}`} key={`${index}:${step.step}`}>{step.status==="completed"?<Check/>:step.status==="inProgress"?<LoaderCircle className="spin"/>:<Circle/>}<span>{step.step}</span></li>)}</ol></div>}
      {items.length>0&&<div className="codex-work-items">{items.map(item=><div key={item.item_id} className={`work-item-${item.status}`}><ItemIcon item={item}/><span>{item.label}</span></div>)}</div>}
    </div>
  </details>
}

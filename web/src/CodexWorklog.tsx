import { useLayoutEffect, useRef } from "react"
import { CheckCircle2, Circle, ListChecks, LoaderCircle } from "lucide-react"
import { ConversationScrollController } from "./conversation-scroll"
import type { CodexWorklog as Worklog } from "./types"

export function hasVisibleCodexWorklog(worklog:Worklog|undefined):boolean{
  return Boolean(worklog?.summaries.some(item=>item.text.trim())||worklog?.plan?.steps.length)
}

export function CodexWorklog({worklog,active}:{worklog:Worklog;active:boolean}){
  const latest=[...worklog.summaries].reverse().find(item=>item.text.trim())
  const plan=worklog.plan
  const viewportRef=useRef<HTMLDivElement>(null)
  const controllerRef=useRef(new ConversationScrollController(()=>viewportRef.current))
  const positionedRef=useRef(false)

  useLayoutEffect(()=>{
    if(!active||(!latest&&!plan?.steps.length))return
    if(!positionedRef.current){
      controllerRef.current.positionInitial()
      positionedRef.current=true
      return
    }
    controllerRef.current.followContent("auto")
  },[active,latest?.text,plan])

  if(!active)return null
  if(!latest&&!plan?.steps.length)return null
  const completed=plan?.steps.filter(step=>step.status==="completed").length??0
  return <div
    className="codex-native-work codex-native-work-scroll"
    ref={viewportRef}
    role="status"
    aria-label="Codex 工作过程"
    aria-live="polite"
    tabIndex={0}
    onScroll={()=>controllerRef.current.handleScroll()}
  >
    {plan&&plan.steps.length>0&&<details className="codex-plan" open>
      <summary><ListChecks/><span>执行计划</span><small>{completed}/{plan.steps.length}</small></summary>
      {plan.explanation&&<p>{plan.explanation}</p>}
      <ol>{plan.steps.map((step,index)=><li className={`plan-${step.status}`} key={`${index}:${step.step}`}>{step.status==="completed"?<CheckCircle2/>:step.status==="inProgress"?<LoaderCircle className="spin"/>:<Circle/>}<span>{step.step}</span></li>)}</ol>
    </details>}
    {latest&&<div className="codex-latest-thought"><LoaderCircle className="spin"/><p>{latest.text}</p></div>}
  </div>
}

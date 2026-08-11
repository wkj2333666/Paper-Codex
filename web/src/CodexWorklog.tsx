import { CheckCircle2, Circle, ListChecks, LoaderCircle } from "lucide-react"
import type { CodexWorklog as Worklog } from "./types"

export function CodexWorklog({worklog,active}:{worklog:Worklog;active:boolean}){
  if(!active)return null
  const latest=[...worklog.summaries].reverse().find(item=>item.text.trim())
  const plan=worklog.plan
  if(!latest&&!plan?.steps.length)return null
  const completed=plan?.steps.filter(step=>step.status==="completed").length??0
  return <div className="codex-native-work" role="status" aria-live="polite">
    {latest&&<div className="codex-latest-thought"><LoaderCircle className="spin"/><p>{latest.text}</p></div>}
    {plan&&plan.steps.length>0&&<details className="codex-plan" open>
      <summary><ListChecks/><span>执行计划</span><small>{completed}/{plan.steps.length}</small></summary>
      {plan.explanation&&<p>{plan.explanation}</p>}
      <ol>{plan.steps.map((step,index)=><li className={`plan-${step.status}`} key={`${index}:${step.step}`}>{step.status==="completed"?<CheckCircle2/>:step.status==="inProgress"?<LoaderCircle className="spin"/>:<Circle/>}<span>{step.step}</span></li>)}</ol>
    </details>}
  </div>
}

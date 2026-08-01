import { CircleStop, Pause, Pencil, Play, X } from "lucide-react"
import type { CodexGoal } from "./types"

const number = new Intl.NumberFormat("en-US")

export function CodexGoalBar({goal,onPause,onResume,onEdit,onClear}:{goal:CodexGoal;onPause:()=>void;onResume:()=>void;onEdit:()=>void;onClear:()=>void}){
  const active=goal.status==="active"
  const terminal=["complete","completed","blocked","failed","cancelled"].includes(goal.status)
  return <section className={`codex-goal-bar goal-${goal.status}`} aria-label="Codex 目标">
    <span className="codex-goal-icon"><CircleStop/></span>
    <div className="codex-goal-copy"><strong>{goal.objective}</strong><span>{goal.token_budget?`${number.format(goal.tokens_used)} / ${number.format(goal.token_budget)} tokens`: `${number.format(goal.tokens_used)} tokens`} · {Math.max(0,goal.time_used_seconds)} 秒</span></div>
    <div className="codex-goal-actions">
      {!terminal&&(active?<button aria-label="暂停目标" title="暂停目标" onClick={onPause}><Pause/></button>:<button aria-label="继续目标" title="继续目标" onClick={onResume}><Play/></button>)}
      {!terminal&&<button aria-label="编辑目标" title="编辑目标" onClick={onEdit}><Pencil/></button>}
      <button aria-label="清除目标" title="清除目标" onClick={onClear}><X/></button>
    </div>
  </section>
}

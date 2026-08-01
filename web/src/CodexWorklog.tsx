import { LoaderCircle } from "lucide-react"
import type { CodexWorklog as Worklog } from "./types"

export function CodexWorklog({worklog,active}:{worklog:Worklog;active:boolean}){
  if(!active)return null
  const latest=[...worklog.summaries].reverse().find(item=>item.text.trim())
  if(!latest)return null
  return <div className="codex-latest-thought" role="status" aria-live="polite">
    <LoaderCircle className="spin"/><p>{latest.text}</p>
  </div>
}

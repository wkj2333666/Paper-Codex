import type { Activity, Dashboard, StreamEvent } from "./types"
import { intakeStateLabel } from "./intake-status"

export interface AppStreamState { activities:Activity[]; latestAnswer:string|null; lastEventId:number }
export const initialState:AppStreamState = { activities:[], latestAnswer:null, lastEventId:0 }

const EVENT_LABELS:Record<string,string>={queued:"任务已加入队列",answer:"Codex 已回答",result:"论文处理完成",failed:"任务失败"}

export function reduceEvent(state:AppStreamState,event:StreamEvent):AppStreamState {
  const detail = typeof event.payload.state === "string" ? event.payload.state
    : typeof event.payload.message === "string" ? event.payload.message
    : typeof event.payload.title === "string" ? event.payload.title : ""
  const label=event.type==="stage"&&typeof event.payload.state==="string"
    ? intakeStateLabel(event.payload.state)
    : event.type==="model-switch"&&typeof event.payload.from==="string"&&typeof event.payload.to==="string"
      ? `模型容量不足：${event.payload.from} → ${event.payload.to}`
      : event.type==="analysis-warning"&&typeof event.payload.source_key==="string"&&typeof event.payload.relation_type==="string"&&typeof event.payload.target_key==="string"
        ? `已忽略无法定位的图谱关系：${event.payload.source_key} --${event.payload.relation_type}--> ${event.payload.target_key}`
        : event.type==="result"&&typeof event.payload.analysis_model==="string"
          ? `论文处理完成 · ${event.payload.analysis_model}${typeof event.payload.reasoning_effort==="string"?` · ${event.payload.reasoning_effort}`:""}`
          : `${EVENT_LABELS[event.type]??event.type}${detail ? ` · ${detail}` : ""}`
  const activity:Activity = { id:event.id,taskId:event.task_id,type:event.type,label,createdAt:event.created_at }
  return { activities:[activity,...state.activities].slice(0,100),
    latestAnswer:event.type === "answer" && typeof event.payload.text === "string" ? event.payload.text : state.latestAnswer,
    lastEventId:Math.max(state.lastEventId,event.id) }
}

export function projectPaperCount(dashboard:Dashboard,projectId:string):number { return dashboard.project_memberships?.[projectId]?.length ?? 0 }

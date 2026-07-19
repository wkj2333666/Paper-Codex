import type { Activity, Dashboard, StreamEvent } from "./types"

export interface AppStreamState { activities:Activity[]; latestAnswer:string|null; lastEventId:number }
export const initialState:AppStreamState = { activities:[], latestAnswer:null, lastEventId:0 }

const STAGE_LABELS:Record<string,string>={
  queued:"等待处理",resolving:"正在识别论文",fetching:"正在获取论文",extracting:"正在提取正文",
  analyzing:"正在分析论文",staging:"正在整理知识",validating:"正在校验证据",committing:"正在保存笔记",
  indexing:"正在更新检索与图谱",done:"处理完成","needs-input":"等待补充信息",failed:"处理失败",cancelled:"已取消",
}
const EVENT_LABELS:Record<string,string>={queued:"任务已加入队列",answer:"Codex 已回答",result:"论文处理完成",failed:"任务失败"}

export function reduceEvent(state:AppStreamState,event:StreamEvent):AppStreamState {
  const detail = typeof event.payload.state === "string" ? event.payload.state
    : typeof event.payload.message === "string" ? event.payload.message
    : typeof event.payload.title === "string" ? event.payload.title : ""
  const label=event.type==="stage"&&typeof event.payload.state==="string"
    ? STAGE_LABELS[event.payload.state]??`处理阶段 · ${event.payload.state}`
    : `${EVENT_LABELS[event.type]??event.type}${detail ? ` · ${detail}` : ""}`
  const activity:Activity = { id:event.id,taskId:event.task_id,type:event.type,label,createdAt:event.created_at }
  return { activities:[activity,...state.activities].slice(0,100),
    latestAnswer:event.type === "answer" && typeof event.payload.text === "string" ? event.payload.text : state.latestAnswer,
    lastEventId:Math.max(state.lastEventId,event.id) }
}

export function projectPaperCount(dashboard:Dashboard,projectId:string):number { return dashboard.project_memberships?.[projectId]?.length ?? 0 }

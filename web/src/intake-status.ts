import type { StreamEvent, Task } from "./types"

const STATE_LABELS: Record<string, string> = {
  queued: "等待处理",
  resolving: "正在识别论文",
  fetching: "正在获取论文",
  extracting: "正在提取正文",
  analyzing: "正在分析论文",
  staging: "正在整理知识",
  validating: "正在校验证据",
  committing: "正在保存笔记",
  indexing: "正在更新检索与图谱",
  done: "处理完成",
  "needs-input": "等待补充信息",
  failed: "处理失败",
  cancelled: "已取消",
}

export function intakeStateLabel(state: string): string {
  return STATE_LABELS[state] ?? `处理阶段 · ${state}`
}

export function taskSource(task: Task): string {
  try {
    const input = JSON.parse(task.input_json) as { source?: unknown }
    if (typeof input.source === "string" && input.source.trim()) return input.source.trim()
  } catch {
    // A malformed task remains visible with a safe fallback label.
  }
  return "未命名论文"
}

export function mergeIntakeTaskEvent(tasks: Task[], event: StreamEvent): Task[] {
  return tasks.map(task => {
    if (task.id !== event.task_id || task.kind !== "ingest") return task
    if (event.type === "stage" && typeof event.payload.state === "string") {
      return { ...task, state: event.payload.state, updated_at: event.created_at || task.updated_at }
    }
    if (event.type === "failed") {
      return {
        ...task,
        state: "failed",
        error: typeof event.payload.message === "string" ? event.payload.message : task.error,
        updated_at: event.created_at || task.updated_at,
      }
    }
    if (event.type === "cancelled") return { ...task, state: "cancelled", updated_at: event.created_at || task.updated_at }
    if (event.type === "result" || event.type === "done") return { ...task, state: "done", updated_at: event.created_at || task.updated_at }
    return task
  })
}

export function groupIntakeTasks(tasks: Task[]): { active: Task[]; failed: Task[] } {
  const imports = tasks.filter(task => task.kind === "ingest")
  const newestFirst = (left: Task, right: Task) => right.created_at.localeCompare(left.created_at)
  return {
    active: imports.filter(task => !["done", "failed", "cancelled"].includes(task.state)).sort(newestFirst),
    failed: imports.filter(task => ["failed", "cancelled"].includes(task.state)).sort(newestFirst),
  }
}

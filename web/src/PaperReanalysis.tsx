import { useEffect, useRef, useState } from "react"
import { LoaderCircle, Sparkles } from "lucide-react"
import { api } from "./api"
import { intakeStateLabel } from "./intake-status"
import { analysisActive, analysisErrorMessage, latestPaperAnalysis, originalPaperUrl } from "./paper-actions"
import type { Paper, Task } from "./types"

export function PaperReanalysis({ paper, tasks, onComplete }: { paper: Paper; tasks: Task[]; onComplete: () => Promise<void> }) {
  const [tracked, setTracked] = useState<Task | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState("")
  const [pollError, setPollError] = useState("")
  const submittingRef = useRef(false)
  const refreshed = useRef<string | null>(null)
  // A locally fetched task replaces its stale dashboard copy.
  const task = latestPaperAnalysis(tracked ? [...tasks.filter(item => item.id !== tracked.id), tracked] : tasks, paper)
  const active = analysisActive(task)
  const source = originalPaperUrl(paper)
  useEffect(() => {
    if (!task || !active) return
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const id = task.id
    const poll = async () => {
      let delay = 3000
      try {
        const next = await api.task(id)
        if (stopped) return
        setTracked(next); setPollError("")
        if (!analysisActive(next)) return
      } catch {
        if (stopped) return
        setPollError("暂时无法读取任务状态，正在恢复连接；不会重复提交分析。")
        delay = 15000
      }
      if (!stopped) timer = setTimeout(() => void poll(), delay)
    }
    void poll()
    return () => { stopped = true; clearTimeout(timer) }
  }, [task?.id, active])
  useEffect(() => {
    if (!task || task.state !== "done" || refreshed.current === task.id) return
    refreshed.current = task.id
    let stopped = false
    void onComplete().catch(() => { if (!stopped) setError("分析已完成，但页面刷新失败，请重新打开论文查看新结果。") })
    return () => { stopped = true }
  }, [task?.id, task?.state, onComplete])
  const submit = async () => {
    if (submittingRef.current || active || !source) return
    submittingRef.current = true; setSubmitting(true); setError(""); setPollError("")
    try {
      // Reanalysis must not silently alter project membership.
      const result = await api.intake(source)
      const now = new Date().toISOString()
      setTracked({ id: result.task_id, kind: "ingest", state: "queued", input_json: JSON.stringify({ source }), paper_id: paper.id, project_id: null, thread_id: null, error: null, created_at: now, updated_at: now })
    } catch (value) { setError(analysisErrorMessage(value instanceof Error ? value.message : "提交分析失败")) }
    finally { submittingRef.current = false; setSubmitting(false) }
  }
  return <div className="paper-reanalysis">
    <button className="outline" disabled={submitting || active || !source} onClick={() => void submit()}>{submitting || active ? <LoaderCircle className="spin"/> : <Sparkles/>}{submitting ? "正在提交…" : active ? intakeStateLabel(task!.state) : "重新分析"}</button>
    {(error || task?.state === "failed") && <p role="alert">{error || analysisErrorMessage(task?.error ?? "分析失败")}</p>}
    {!error && task?.state === "done" && <p role="status">分析已完成</p>}
    {!error && task?.state === "cancelled" && <p role="status">分析已取消，可重新提交。</p>}
    {pollError && active && <p role="status">{pollError}</p>}
    {!source && <p>这篇论文没有原始链接，暂不支持重新获取分析。</p>}
  </div>
}

import { useEffect, useRef, useState } from "react"
import { api } from "./api"
import type { BriefingPaper } from "./MorningBriefing"
import type { Project } from "./types"

export function projectLabel(projects: Project[], id: string): string {
  const names: string[] = []
  const visited = new Set<string>()
  let project = projects.find(project => project.id === id)
  while (project && !visited.has(project.id)) {
    visited.add(project.id); names.unshift(project.name)
    project = projects.find(parent => parent.id === project?.parent_id)
  }
  return names.join(" / ") || id
}
type ImportState = { state: "importing" | "done" | "failed"; taskId?: string; error?: string }
const stateKey = (target: string, paper: string) => JSON.stringify([target, paper])

export function BriefingPaperPicker({ briefingId, ownerId, papers, projects }: { briefingId: string; ownerId: string; papers: BriefingPaper[]; projects: Project[] }) {
  const [target, setTarget] = useState(ownerId)
  const [selected, setSelected] = useState<string[]>([])
  const [states, setStates] = useState<Record<string, ImportState>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const inFlight = useRef(false)
  useEffect(() => {
    let active = true
    void api.projectCandidates(target).then(candidates => {
      if (!active) return
      setStates(previous => {
        const next = { ...previous }
        for (const candidate of candidates) {
          if (!papers.some(paper => paper.key === candidate.work.canonical_key)) continue
          const key = stateKey(target, candidate.work.canonical_key)
          if (candidate.status === "imported") next[key] = { state: "done" }
          else if (candidate.status === "importing" && candidate.import_task_id && !next[key]) next[key] = { state: "importing", taskId: candidate.import_task_id }
        }
        return next
      })
    }).catch(error => { if (active) setError(error instanceof Error ? error.message : "读取导入状态失败") })
    return () => { active = false }
  }, [target, papers])
  useEffect(() => {
    const pending = Object.entries(states).filter(([, value]) => value.state === "importing" && value.taskId)
    if (!pending.length) return
    let active = true
    const timer = setTimeout(() => {
      void Promise.all(pending.map(async ([key, value]) => {
        const task = await api.task(value.taskId!)
        if (!active) return
        if (task.state === "done") setStates(previous => ({ ...previous, [key]: { state: "done" } }))
        else if (["failed", "cancelled", "needs-input"].includes(task.state)) setStates(previous => ({ ...previous, [key]: { state: "failed", error: task.error || "导入未完成，请查看任务详情后重试" } }))
      })).then(() => { if (active) { setError(""); setStates(previous => ({ ...previous })) } }).catch(error => { if (active) { setError(error instanceof Error ? error.message : "读取任务状态失败"); setStates(previous => ({ ...previous })) } })
    }, 5000)
    return () => { active = false; clearTimeout(timer) }
  }, [states])
  const status = (paper: BriefingPaper) => papers.find(value => value.key === paper.key)?.project_ids.includes(target) ? { state: "done" as const } : states[stateKey(target, paper.key)]
  const importSelected = async () => {
    if (inFlight.current) return
    inFlight.current = true; setBusy(true); setError("")
    try {
      for (const paper of papers.filter(paper => selected.includes(paper.key) && !["done", "importing"].includes(status(paper)?.state ?? ""))) {
        const key = stateKey(target, paper.key)
        setStates(previous => ({ ...previous, [key]: { state: "importing" } }))
        try {
          const result = await api.importBriefingPaper(briefingId, paper.key, target)
          setStates(previous => ({ ...previous, [key]: result.state === "existing" ? { state: "done" } : { state: "importing", taskId: result.task_id } }))
        } catch (error) { setStates(previous => ({ ...previous, [key]: { state: "failed", error: error instanceof Error ? error.message : "导入失败" } })) }
      }
      setSelected([])
    } finally { inFlight.current = false; setBusy(false) }
  }
  if (!papers.length) return null
  return <section className="briefing-paper-picker" aria-label="从晨报选论文加入项目">
    <h2>感兴趣的论文，加入项目</h2>
    <p>只导入你勾选的论文；已有论文直接关联。也可选择 EI 下的具体子项目。</p>
    <label>加入到 <select aria-label="论文目标项目" value={target} disabled={busy} onChange={event => { setTarget(event.target.value); setSelected([]); setError("") }}>{projects.map(project => <option key={project.id} value={project.id}>{projectLabel(projects, project.id)}</option>)}</select></label>
    {error && <p role="alert">{error}</p>}
    <ul>{papers.map(paper => {
      const current = status(paper)
      return <li key={paper.key}><label><input type="checkbox" checked={selected.includes(paper.key)} disabled={busy || current?.state === "done" || current?.state === "importing"} onChange={event => setSelected(previous => event.target.checked ? [...previous, paper.key] : previous.filter(key => key !== paper.key))}/><span><a href={paper.source_url} target="_blank" rel="noreferrer">{paper.title}</a><small>{paper.authors.slice(0, 3).join(", ")}{paper.authors.length > 3 ? " 等" : ""}{paper.year ? ` · ${paper.year}` : ""}</small></span></label>{current && <p role={current.state === "failed" ? "alert" : "status"}>{current.state === "done" ? "已加入" : current.state === "importing" ? "正在导入…" : current.error}</p>}</li>
    })}</ul>
    <button disabled={busy || !selected.length || !projects.some(project => project.id === target)} onClick={() => void importSelected()}>加入所选论文（{selected.length}）</button>
  </section>
}

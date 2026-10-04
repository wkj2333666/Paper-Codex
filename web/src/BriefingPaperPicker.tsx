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
export function suggestedTarget(paper: BriefingPaper, ownerId: string, projects: Project[]): string {
  const suggestions = (paper.suggested_projects ?? []).filter(item => projects.some(project => project.id === item.project_id))
  const best = Math.max(0, ...suggestions.map(item => item.score))
  const top = suggestions.filter(item => item.score === best)
  if (top.length === 1) return top[0].project_id
  return !suggestions.length && !projects.some(project => project.parent_id === ownerId) && projects.some(project => project.id === ownerId) ? ownerId : ""
}
export function analysisLabel(paper: BriefingPaper): string {
  const analysis = paper.analysis
  if (!paper.paper_id) return ""
  if (analysis?.state === "running") return "已入库 · Codex 描述正在生成"
  if (analysis?.state === "failed") {
    const reason = analysis.error?.includes("only allows Codex official clients")
      ? "模型服务拒绝此客户端（403）；请先检查服务商访问权限，重复导入不会解决"
      : analysis.error || "请查看任务记录"
    return `已入库 · ${analysis.has_description ? "重新分析失败，保留已有描述" : "Codex 描述生成失败"}：${reason}`
  }
  return analysis?.has_description ? "Codex 描述已生成" : "已入库 · 尚无 Codex 描述"
}
type ImportState = { state: "importing" | "done" | "failed"; taskId?: string; error?: string }
const stateKey = (target: string, paper: string) => JSON.stringify([target, paper])

export function BriefingPaperPicker({ briefingId, ownerId, papers, projects }: { briefingId: string; ownerId: string; papers: BriefingPaper[]; projects: Project[] }) {
  const [livePapers, setLivePapers] = useState(papers)
  const [targets, setTargets] = useState<Record<string, string>>({})
  const [selected, setSelected] = useState<string[]>([])
  const [states, setStates] = useState<Record<string, ImportState>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const inFlight = useRef(false)
  useEffect(() => { setLivePapers(papers) }, [papers])
  const targetFor = (paper: BriefingPaper) => targets[paper.key] ?? suggestedTarget(paper, ownerId, projects)
  const targetIds = JSON.stringify([...new Set(livePapers.map(targetFor).filter(Boolean))].sort())
  const paperKeys = JSON.stringify(papers.map(paper => paper.key))
  useEffect(() => {
    let active = true
    void Promise.all((JSON.parse(targetIds) as string[]).map(async target => {
      const candidates = await api.projectCandidates(target)
      if (!active) return
      setStates(previous => {
        const next = { ...previous }
        for (const candidate of candidates) {
          if (!(JSON.parse(paperKeys) as string[]).includes(candidate.work.canonical_key)) continue
          const key = stateKey(target, candidate.work.canonical_key)
          if (candidate.status === "importing" && candidate.import_task_id && !next[key]) next[key] = { state: "importing", taskId: candidate.import_task_id }
          // Imported means membership, not successful analysis.
        }
        return next
      })
    })).catch(error => { if (active) setError(error instanceof Error ? error.message : "读取导入状态失败") })
    return () => { active = false }
  }, [targetIds, paperKeys])
  useEffect(() => {
    const pending = Object.entries(states).filter(([, value]) => value.state === "importing" && value.taskId)
    if (!pending.length && !livePapers.some(paper => paper.analysis?.state === "running")) return
    let active = true
    const timer = setTimeout(() => {
      void (async () => {
        const results = await Promise.all(pending.map(async ([key, value]) => [key, await api.task(value.taskId!)] as const))
        const detail = await api.briefing(briefingId)
        if (!active) return
        if (detail.papers) setLivePapers(detail.papers)
        setStates(previous => {
          const next = { ...previous }
          for (const [key, task] of results) {
            if (task.state === "done") next[key] = { state: "done" }
            else if (["failed", "cancelled", "needs-input"].includes(task.state)) next[key] = { state: "failed", error: task.error || "任务未完成" }
          }
          return next
        })
        setError("")
      })().catch(error => { if (active) { setError(error instanceof Error ? error.message : "读取任务状态失败"); setStates(previous => ({ ...previous })) } })
    }, 5000)
    return () => { active = false; clearTimeout(timer) }
  }, [states, livePapers, briefingId])
  const status = (paper: BriefingPaper) => states[stateKey(targetFor(paper), paper.key)]
  const added = (paper: BriefingPaper) => paper.project_ids.includes(targetFor(paper)) || status(paper)?.state === "done"
  const importSelected = async () => {
    if (inFlight.current) return
    inFlight.current = true; setBusy(true); setError("")
    try {
      for (const paper of livePapers.filter(paper => selected.includes(paper.key) && !added(paper) && status(paper)?.state !== "importing")) {
        const target = targetFor(paper)
        if (!projects.some(project => project.id === target)) continue
        const key = stateKey(target, paper.key)
        setStates(previous => ({ ...previous, [key]: { state: "importing" } }))
        try {
          const result = await api.importBriefingPaper(briefingId, paper.key, target)
          setStates(previous => ({ ...previous, [key]: result.state === "existing" ? { state: "done" } : { state: "importing", taskId: result.task_id } }))
        } catch (error) { setStates(previous => ({ ...previous, [key]: { state: "failed", error: error instanceof Error ? error.message : "导入失败" } })) }
      }
      const detail = await api.briefing(briefingId)
      if (detail.papers) setLivePapers(detail.papers)
      setSelected([])
    } catch (error) { setError(error instanceof Error ? error.message : "刷新状态失败") }
    finally { inFlight.current = false; setBusy(false) }
  }
  const reanalyze = async (paper: BriefingPaper) => {
    if (inFlight.current) return
    inFlight.current = true; setBusy(true); setError("")
    try {
      const result = await api.intake(paper.source_url, paper.project_ids[0])
      setStates(previous => ({ ...previous, [`analysis:${paper.key}`]: { state: "importing", taskId: result.task_id } }))
    } catch (error) { setError(error instanceof Error ? error.message : "提交分析失败") }
    finally { inFlight.current = false; setBusy(false) }
  }
  if (!livePapers.length) return null
  const needsChoice = livePapers.some(paper => selected.includes(paper.key) && !projects.some(project => project.id === targetFor(paper)))
  return <section className="briefing-paper-picker" aria-label="从晨报选论文加入项目">
    <h2>感兴趣的论文，加入项目</h2>
    <p>按论文研究主题逐篇推荐项目；可以修改。方向不明确时，请先选择项目。入库与 Codex 描述生成分别显示。</p>
    {error && <p role="alert">{error}</p>}
    <ul>{livePapers.map(paper => {
      const current = status(paper)
      const target = targetFor(paper)
      const pending = current?.state === "importing" || states[`analysis:${paper.key}`]?.state === "importing" || paper.analysis?.state === "running"
      return <li key={paper.key}>
        <label><input type="checkbox" checked={selected.includes(paper.key)} disabled={busy || added(paper) || pending} onChange={event => setSelected(previous => event.target.checked ? [...previous, paper.key] : previous.filter(key => key !== paper.key))}/><span><a href={paper.source_url} target="_blank" rel="noreferrer">{paper.title}</a><small>{paper.authors.slice(0, 3).join(", ")}{paper.authors.length > 3 ? " 等" : ""}{paper.year ? ` · ${paper.year}` : ""}</small></span></label>
        <label className="briefing-paper-target">加入到 <select aria-label={`${paper.title} 的目标项目`} value={target} disabled={busy || pending} onChange={event => setTargets(previous => ({...previous, [paper.key]:event.target.value}))}>
          <option value="">请选择合适的项目</option>{projects.map(project => <option key={project.id} value={project.id}>{projectLabel(projects, project.id)}{paper.suggested_projects?.some(item => item.project_id === project.id) ? " · 推荐" : ""}</option>)}
        </select></label>
        {!!paper.suggested_projects?.length && <small>匹配主题：{[...new Set(paper.suggested_projects.map(item => item.reason))].join("；")}（建议，可修改）</small>}
        {paper.project_ids.length > 0 && <small>已加入：{paper.project_ids.map(id => projectLabel(projects, id)).join("；")}</small>}
        {paper.paper_id && <p role={paper.analysis?.state === "failed" ? "alert" : "status"}>{analysisLabel(paper)}</p>}
        {pending && <p role="status">正在处理导入／分析任务…</p>}
        {current?.state === "failed" && !paper.paper_id && <p role="alert">{current.error}</p>}
        {paper.paper_id && paper.analysis?.state !== "ready" && <button disabled={busy || pending} onClick={() => void reanalyze(paper)}>重新分析</button>}
      </li>
    })}</ul>
    {needsChoice && <p role="status">请为已勾选的论文选择目标项目后再加入。</p>}
    <button disabled={busy || !selected.length || needsChoice} onClick={() => void importSelected()}>加入所选论文（{selected.length}）</button>
  </section>
}

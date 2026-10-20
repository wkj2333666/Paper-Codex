import { describe, expect, it } from "vitest"
import { renderToStaticMarkup } from "react-dom/server"
import { analysisActive, analysisErrorMessage, latestPaperAnalysis, originalPaperUrl } from "./paper-actions"
import { PaperReanalysis } from "./PaperReanalysis"
import type { Paper, Task } from "./types"

const paper: Paper = { id: "arxiv:2606.03784", title: "Revisiting Embodied Chain-of-Thought", source_url: "https://arxiv.org/abs/2606.03784", arxiv_id: "2606.03784", doi: null, authors_json: "[]", year: 2026, canonical_sha256: "revision", note_path: null, deleted_at: null, created_at: "2026-10-20", updated_at: "2026-10-20" }
const task = (state: string, overrides: Partial<Task> = {}): Task => ({ id: "task", kind: "ingest", state, input_json: JSON.stringify({ source: paper.source_url }), paper_id: paper.id, project_id: null, thread_id: null, error: null, created_at: "2026-10-20 03:08:55", updated_at: "2026-10-20 03:10:38", ...overrides })

describe("paper actions", () => {
  it("uses the original public source and falls back to stable identifiers", () => {
    expect(originalPaperUrl(paper)).toBe(paper.source_url)
    expect(originalPaperUrl({ ...paper, source_url: "javascript:alert(1)" })).toBe(paper.source_url)
    expect(originalPaperUrl({ source_url: "https://user:secret@example.com/a", arxiv_id: null, doi: "10.1/test" })).toBe("https://doi.org/10.1/test")
    expect(originalPaperUrl({ source_url: "file:///tmp/paper.pdf", arxiv_id: null, doi: null })).toBeNull()
  })
  it("tracks queued tasks before a paper id is assigned and excludes other papers", () => {
    const queued = task("queued", { paper_id: null })
    expect(latestPaperAnalysis([queued], paper)).toBe(queued)
    expect(latestPaperAnalysis([task("analyzing", { paper_id: "other" })], paper)).toBeNull()
    expect(latestPaperAnalysis([task("queued", { paper_id: null, input_json: "invalid" })], paper)).toBeNull()
  })
  it("keeps the action busy for any unfinished task, not just the submit request", () => {
    const running = task("analyzing", { id: "active" })
    const failed = task("failed", { created_at: "2026-10-20T04:00:00Z" })
    expect(latestPaperAnalysis([failed, running], paper)).toBe(running)
    expect(analysisActive(running)).toBe(true)
    expect(analysisActive(failed)).toBe(false)
    const html = renderToStaticMarkup(<PaperReanalysis paper={paper} tasks={[running]} onComplete={async () => {}}/> )
    expect(html).toContain('disabled=""')
    expect(html).toContain("正在分析论文")
  })
  it("does not let stale polling hide a terminal SSE event", () => {
    const stale = task("analyzing")
    const completed = task("done", { updated_at: "2026-10-20T03:11:00Z" })
    expect(latestPaperAnalysis([completed, stale], paper)).toBe(completed)
    expect(latestPaperAnalysis([stale, completed], paper)).toBe(completed)
    expect(analysisActive(latestPaperAnalysis([stale, completed], paper))).toBe(false)
  })
  it("shows provider rejection on the paper page without credentials or raw transport output", () => {
    const error = '403 Forbidden: This account only allows Codex official clients; sk-secret123 event: response.failed'
    const html = renderToStaticMarkup(<PaperReanalysis paper={paper} tasks={[task("failed", { error })]} onComplete={async () => {}}/> )
    expect(html).toContain('role="alert"')
    expect(html).toContain("该账号只允许 Codex 官方客户端")
    expect(html).not.toContain("sk-secret123")
    expect(html).not.toContain("response.failed")
    expect(analysisErrorMessage("failed sk-testkey123 Bearer abcdef")).not.toContain("testkey123")
  })
})

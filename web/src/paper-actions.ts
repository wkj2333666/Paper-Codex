import type { Paper, Task } from "./types"

export function originalPaperUrl(paper: Pick<Paper, "source_url" | "arxiv_id" | "doi">): string | null {
  if (paper.source_url) {
    try {
      const url = new URL(paper.source_url)
      if (["https:", "http:"].includes(url.protocol) && !url.username && !url.password) return url.href
    } catch { /* Uploaded papers may have no public source. */ }
  }
  if (paper.arxiv_id) return `https://arxiv.org/abs/${encodeURIComponent(paper.arxiv_id).replace(/%2F/gi, "/")}`
  if (paper.doi) return `https://doi.org/${encodeURIComponent(paper.doi).replace(/%2F/gi, "/")}`
  return null
}

export function analysisActive(task: Task | null): boolean {
  return !!task && !["done", "failed", "cancelled"].includes(task.state)
}

export function latestPaperAnalysis(tasks: Task[], paper: Paper): Task | null {
  const timestamp = (value: string) => Date.parse(value.includes("T") ? value : value.replace(" ", "T") + "Z") || 0
  const current = new Map<string, Task>()
  for (const task of tasks) {
    const previous = current.get(task.id)
    if (!previous || timestamp(task.updated_at) >= timestamp(previous.updated_at)) current.set(task.id, task)
  }
  const sources = new Set([paper.source_url, paper.arxiv_id && `arxiv:${paper.arxiv_id}`, paper.doi && `doi:${paper.doi}`].filter(Boolean))
  const matches = [...current.values()].filter(task => {
    if (task.kind !== "ingest") return false
    if (task.paper_id) return task.paper_id === paper.id
    try { return sources.has(JSON.parse(task.input_json).source) } catch { return false }
  })
  return matches.sort((a, b) => Number(analysisActive(b)) - Number(analysisActive(a)) || timestamp(b.created_at) - timestamp(a.created_at) || timestamp(b.updated_at) - timestamp(a.updated_at))[0] ?? null
}

export function analysisErrorMessage(error: string): string {
  if (/only allows Codex official clients/i.test(error)) return "模型服务拒绝请求（403）：该账号只允许 Codex 官方客户端。请检查分析模型的服务商与账号权限；反复点击不能解决此限制。"
  if (/401|invalid_api_key/i.test(error)) return "模型服务认证失败（401），请检查分析模型的凭据。"
  if (/403|forbidden/i.test(error)) return "服务拒绝访问（403），请检查论文来源或模型服务的访问权限。"
  return error.replace(/sk-[\w*.-]+/g, "[密钥已隐藏]").replace(/Bearer\s+\S+/gi, "Bearer [已隐藏]").slice(0, 350) || "分析失败，请查看任务记录。"
}

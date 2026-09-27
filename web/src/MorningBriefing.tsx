import { useEffect, useState } from "react"
import { api } from "./api"
import { ChatMarkdown } from "./ChatMarkdown"
import type { Project } from "./types"
import "./morning-briefing.css"

export interface BriefingConfig {
  enabled: boolean; time: string; timezone: string; categories: string[]; keywords: string[]
  project_ids: string[]; max_papers: number; fulltext_papers: number; timeout_minutes: number
  email_enabled: boolean; recipient: string
}
export interface Briefing {
  id: string; day: string; status: string; markdown: string; error: string | null
  conversation_id: string | null; mail_status: string; mail_attempts: number; attempts: number; mail_error: string | null
}
export interface BriefingResponse { config: BriefingConfig | null; config_error: string | null; mail_configured: boolean; items: Briefing[] }
const labels: Record<string, string> = { running: "生成中", completed: "已生成", empty: "暂无新论文", failed: "失败", pending: "待发送", sending: "发送中", sent: "已发送", skipped: "未安排发送", uncertain: "发送结果待确认", blocked: "需检查发信配置" }
const message = (error: unknown) => error instanceof Error ? error.message : "操作失败"
const split = (value: string) => value.split(/[,，\n]/).map(word => word.trim()).filter(Boolean)

export function MorningBriefing({ projects }: { projects: Project[] }) {
  const [data, setData] = useState<BriefingResponse | null>(null)
  const [draft, setDraft] = useState<BriefingConfig | null>(null)
  const [selected, setSelected] = useState("")
  const [error, setError] = useState("")
  const [notice, setNotice] = useState("")
  const [busy, setBusy] = useState(false)
  const [detail, setDetail] = useState<Briefing | null>(null)
  const item = data?.items.find(item => item.id === selected) ?? data?.items[0]
  useEffect(() => {
    let active = true
    setDetail(null)
    if (item && ["completed", "empty"].includes(item.status)) {
      void api.briefing(item.id).then(value => { if (active) setDetail(value) }).catch(error => { if (active) setError(message(error)) })
    }
    return () => { active = false }
  }, [item?.id, item?.status])
  useEffect(() => {
    let active = true
    let timer: ReturnType<typeof setTimeout>
    const load = async () => {
      try { const next = await api.briefings(); if (active) { setData(next); setError("") } }
      catch (error) { if (active) setError(message(error)) }
      finally { if (active) timer = setTimeout(() => void load(), 30000) }
    }
    void load()
    return () => { active = false; clearTimeout(timer) }
  }, [])
  const action = async (run: () => Promise<unknown>, success: string) => {
    setBusy(true); setError(""); setNotice("")
    try { await run(); setData(await api.briefings()); setNotice(success) }
    catch (error) { setError(message(error)) }
    finally { setBusy(false) }
  }
  const update = (value: Partial<BriefingConfig>) => setDraft(current => current ? { ...current, ...value } : current)
  return <section className="morning-briefing" aria-label="论文晨报">
    <header><div><h2>论文晨报</h2><p>{data?.config?.enabled ? `每天 ${data.config.time} · 北京时间` : "定时生成未开启"} · {data?.config?.email_enabled ? "邮件已启用" : "站内阅读"}</p></div>
      <div className="briefing-actions"><button disabled={!data?.config || busy} onClick={() => setDraft(draft ? null : data?.config ?? null)}>设置</button><button disabled={busy || !data?.config || data.items.some(item => item.status === "running")} onClick={() => void action(async () => { const result = await api.runBriefing(); setSelected(result.id) }, "已提交；同一天复用已有晨报，生成失败最多尝试 3 次。")}>生成今日晨报</button></div>
    </header>
    {(error || data?.config_error) && <p role="alert">{error || data?.config_error}</p>}
    {notice && <p role="status">{notice}</p>}
    {draft && <form className="briefing-settings" onSubmit={event => { event.preventDefault(); const fields = new FormData(event.currentTarget); const config = { ...draft, categories: split(String(fields.get("categories") ?? "")), keywords: split(String(fields.get("keywords") ?? "")) }; void action(async () => { await api.saveBriefingConfig(config); setDraft(null) }, "设置已保存，无需重启。") }}>
      <label><input type="checkbox" checked={draft.enabled} onChange={e => update({ enabled: e.target.checked })}/>开启每日生成</label>
      <label>北京时间<input type="time" required value={draft.time} onChange={e => update({ time: e.target.value })}/></label>
      <label>arXiv 分类（逗号分隔）<input name="categories" required defaultValue={draft.categories.join(", ")}/></label>
      <label>关注词（逗号分隔）<textarea name="keywords" required defaultValue={draft.keywords.join(", ")}/></label>
      <label>最多论文数<input type="number" min="1" max="20" value={draft.max_papers} onChange={e => update({ max_papers: Number(e.target.value) })}/></label>
      <label>重点阅读全文数<input type="number" min="0" max={Math.min(5, draft.max_papers)} value={draft.fulltext_papers} onChange={e => update({ fulltext_papers: Number(e.target.value) })}/></label>
      <fieldset><legend>参考项目目标与已保存兴趣</legend>{projects.length ? projects.map(project => <label key={project.id}><input type="checkbox" checked={draft.project_ids.includes(project.id)} onChange={e => update({ project_ids: e.target.checked ? [...draft.project_ids, project.id] : draft.project_ids.filter(id => id !== project.id) })}/>{project.name}</label>) : <span>暂无项目；仍参考全局兴趣。</span>}</fieldset>
      <label><input type="checkbox" checked={draft.email_enabled} disabled={!data?.mail_configured} onChange={e => update({ email_enabled: e.target.checked })}/>邮件投递{!data?.mail_configured && "（服务端尚未设置凭据文件）"}</label>
      <label>收件邮箱<input type="email" required={draft.email_enabled} value={draft.recipient} onChange={e => update({ recipient: e.target.value })}/></label>
      <p>无需保持网页开启。没有新论文不发邮件；邮件失败不会重新生成晨报。SMTP 密码仅从服务器本地文件读取。</p>
      <button disabled={busy} type="submit">保存设置</button>
    </form>}
    {item ? <><div className="briefing-actions"><select aria-label="晨报日期" value={item.id} onChange={e => setSelected(e.target.value)}>{data?.items.map(entry => <option key={entry.id} value={entry.id}>{entry.day} · {labels[entry.status] ?? entry.status}</option>)}</select><span>生成：{labels[item.status] ?? item.status} · 邮件：{labels[item.mail_status] ?? item.mail_status}</span>
      {item.status === "completed" && !["sent", "sending"].includes(item.mail_status) && <button disabled={busy || !data?.config?.email_enabled || item.mail_attempts >= 3} onClick={() => { if (item.mail_status === "uncertain" && !window.confirm("上次邮件可能已送达。确认检查邮箱后仍要重发？")) return; void action(() => api.sendBriefing(item.id), "已提交发送请求，请查看发送状态。") }}>发送邮件</button>}</div>
      {item.error && <p role="alert">{item.error}</p>}{item.mail_error && <p role="alert">{item.mail_error}</p>}
      {item.status === "running" && <p role="status">正在检索和整理论文，可离开页面，完成后会保存在这里。</p>}
      {detail?.id === item.id && detail.markdown && <details open><summary>{item.day} 晨报正文</summary><div className="briefing-content chat-markdown"><ChatMarkdown>{detail.markdown}</ChatMarkdown></div></details>}
      {item.conversation_id && <p>可在 Codex 历史对话中打开「{item.day} 论文晨报」继续追问。</p>}
    </> : <p>还没有晨报。设置关注方向后，可手动生成第一份。</p>}
  </section>
}

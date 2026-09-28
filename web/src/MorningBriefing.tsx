import { useEffect, useState } from "react"
import { api } from "./api"
import { ChatMarkdown } from "./ChatMarkdown"
import type { Project } from "./types"
import "./morning-briefing.css"
import { BriefingPaperPicker, projectLabel } from "./BriefingPaperPicker"

export interface BriefingConfig {
  enabled: boolean; time: string; timezone: string; categories: string[]; keywords: string[]
  project_id: string; max_papers: number; fulltext_papers: number; timeout_minutes: number
  email_enabled: boolean; recipient: string
}
export interface Briefing {
  id: string; project_id: string; day: string; status: string; markdown: string; error: string | null
  conversation_id: string | null; mail_status: string; mail_attempts: number; attempts: number; mail_error: string | null
  email_html?: string
  next_attempt_at?: string | null
  search_diagnostics?: BriefingDiagnostics | null
  papers?: BriefingPaper[]
  search_plan?: { terms: string[]; rationale: string; query: string; topics?: {id:string;label:string;intent:string;project_ids:string[];terms:string[]}[] } | null
}
interface RetrievalAudit { received: number; within_window: number; before_window: number; pages: number; complete: boolean; limit_reached: boolean; latest_updated: string | null; request_timeout_seconds?:number; last_request_ms?:number; error_kind?:string|null }
interface SelectionAudit { retrieved: number; unseen: number; candidates: number }
export interface BriefingDiagnostics { stage?: string; since?: string; primary?: RetrievalAudit; fallback?: RetrievalAudit; primary_selection?: SelectionAudit; fallback_selection?: SelectionAudit; selected_candidates?: number; empty_reviewed?: boolean; fallback_reason?:string; active_topic?:string; coverage?:{id:string;label:string;candidates:number;selected:number;status:string}[]; publication_coverage?:{id:string;published:number}[]; dispositions?:{project_id:string;kind:string;reason:string}[]; topic_retrievals?:{id:string;label:string;status:string;pages:number;within_window?:number;error_kind?:string}[] }
export function BriefingSearchDiagnostics({ diagnostics }: { diagnostics: BriefingDiagnostics }) {
  const errors:Record<string,string>={timeout:"请求超时",connection:"连接失败",upstream_5xx:"上游服务异常",http_rejected:"上游拒绝请求",invalid_response:"响应格式异常"}
  const stages: Record<string,string> = { planning: "规划检索", retrieving: "项目检索", fallback_retrieval: "主题请求失败，改用分类检索", empty_review: "复查空结果", collecting_evidence: "读取论文证据", writing: "撰写晨报", verified_empty: "复查后无新增", completed: "已完成", failed: "未完成，不能视作无新增" }
  return <details open><summary>检索记录</summary><p>阶段：{stages[diagnostics.stage ?? ""] ?? "未知"}{diagnostics.since && ` · 窗口起点：${new Date(diagnostics.since).toLocaleString("zh-CN", { timeZone: "Asia/Shanghai" })}（北京时间）`}</p>{(["primary", "fallback"] as const).map(key => {
    const audit = diagnostics[key], selection = diagnostics[key === "primary" ? "primary_selection" : "fallback_selection"]
    return audit && <p key={key}>{key === "primary" ? "项目主题检索" : "分类范围复查"}：收到 {audit.received} 篇 · 窗口内 {audit.within_window} 篇{selection && ` · 去重后 ${selection.unseen} 篇 · 候选 ${selection.candidates} 篇`} · {audit.complete ? "覆盖已确认" : audit.limit_reached ? "达到上限，未完整覆盖" : "尚未完成"}{audit.error_kind && ` · ${errors[audit.error_kind]??"请求失败"}`}{audit.last_request_ms!==undefined && ` · 最近请求 ${(audit.last_request_ms/1000).toFixed(1)} 秒`}{audit.latest_updated && ` · 最新返回记录：${new Date(audit.latest_updated).toLocaleString("zh-CN", { timeZone: "Asia/Shanghai" })}`}</p>
  })}{diagnostics.stage==="retrieving"&&diagnostics.active_topic&&<p>当前主题：{diagnostics.active_topic}</p>}
  {diagnostics.topic_retrievals?.map(topic=><p key={topic.id}>{topic.label}：{topic.status==="complete"?`完成主题检索，窗口内 ${topic.within_window??0} 条`:`主题检索未完成：${errors[topic.error_kind??""]??"请求失败"}`} · {topic.pages} 页</p>)}
  {diagnostics.coverage&&<section aria-label="研究方向覆盖"><h3>研究方向覆盖</h3>{diagnostics.coverage.map(topic=>{const published=diagnostics.publication_coverage?.find(item=>item.id===topic.id)?.published;return <p key={topic.id}><strong>{topic.label}</strong>：{topic.candidates} 篇未介绍候选 · {topic.selected} 篇送交编辑{published!==undefined&&` · 正文提及 ${published} 篇`}{topic.status==="budget_limited"&&" · 阅读预算未选入，不等于没有新增"}{topic.status==="no_unseen_candidates"&&" · 本次未选出未介绍的相关候选"}</p>})}</section>}
  {diagnostics.dispositions?.filter(item=>item.kind!=="organizational").map(item=><p key={item.project_id}>范围待说明：{item.reason}</p>)}
  {diagnostics.empty_reviewed && <p>已执行独立分类检索复查；无新增只表示当前窗口内未选出未介绍的相关论文，不表示没有论文发表。</p>}</details>
}
export const canNotifyBriefing = (item: Briefing) => ["completed", "empty"].includes(item.status) || (item.status === "failed" && (item.attempts >= 3 || !item.next_attempt_at))
export interface BriefingPaper { key: string; title: string; authors: string[]; source_url: string; year: number | null; paper_id: string | null; project_ids: string[] }
export interface BriefingResponse { configs: BriefingConfig[]; config_error: string | null; mail_configured: boolean; items: Briefing[] }
export const newBriefingConfig = (projectId: string): BriefingConfig => ({ project_id: projectId, enabled: false, time: "08:00", timezone: "Asia/Shanghai", categories: ["cs.RO", "cs.CV", "cs.AI"], keywords: [], max_papers: 12, fulltext_papers: 4, timeout_minutes: 15, email_enabled: false, recipient: "" })
const labels: Record<string, string> = { running: "生成中", completed: "已生成", empty: "暂无新论文", failed: "失败", pending: "待发送", sending: "发送中", sent: "已发送", skipped: "未安排发送", uncertain: "发送结果待确认", blocked: "需检查发信配置" }
const message = (error: unknown) => error instanceof Error ? error.message : "操作失败"
const split = (value: string) => value.split(/[,，\n]/).map(word => word.trim()).filter(Boolean)

export function BriefingEmailPreview({ html }: { html: string }) {
  return <iframe className="briefing-email-preview" title="HTML 邮件预览" srcDoc={html} sandbox="allow-popups allow-popups-to-escape-sandbox" referrerPolicy="no-referrer" loading="lazy"/>
}

export function MorningBriefing({ projects }: { projects: Project[] }) {
  const [data, setData] = useState<BriefingResponse | null>(null)
  const [draft, setDraft] = useState<BriefingConfig | null>(null)
  const [selected, setSelected] = useState("")
  const [error, setError] = useState("")
  const [notice, setNotice] = useState("")
  const [busy, setBusy] = useState(false)
  const [detail, setDetail] = useState<Briefing | null>(null)
  const [chosenProject, setChosenProject] = useState("")
  const projectId = chosenProject || data?.configs.find(config => projects.some(project => project.id === config.project_id))?.project_id || projects[0]?.id || ""
  const config = data?.configs.find(config => config.project_id === projectId)
  const items = data?.items.filter(entry => entry.project_id === projectId) ?? []
  const item = items.find(entry => entry.id === selected) ?? items[0]
  const progressRefresh = item?.status === "running" ? data : null
  useEffect(() => {
    let active = true
    setDetail(null)
    if (item) {
      void api.briefing(item.id).then(value => { if (active) setDetail(value) }).catch(error => { if (active) setError(message(error)) })
    }
    return () => { active = false }
  }, [item?.id, item?.status, item?.attempts, item?.error, progressRefresh])
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
    <header><div><h1>论文晨报</h1><p>{config?.enabled ? `每天 ${config.time} · 北京时间` : "定时生成未开启"} · {config?.email_enabled ? "邮件已启用" : "站内阅读"}</p></div>
      <div className="briefing-actions"><button disabled={!data || !projectId || busy} onClick={() => setDraft(draft ? null : config ?? newBriefingConfig(projectId))}>{config ? "设置" : "配置这个项目的晨报"}</button><button disabled={busy || !config || data?.items.some(item => item.status === "running")} onClick={() => void action(async () => { const result = await api.runBriefing(projectId); setSelected(result.id) }, "已提交；已完成内容不重复生成。自动尝试每天最多 3 次，失败后可手动补跑一次；历史失败记录会保留。")}>{item?.status === "failed" ? "手动补跑今日晨报" : item?.status === "empty" && item.mail_attempts === 0 ? "重新检索今日晨报" : "生成今日晨报"}</button></div>
    </header>
    <label className="briefing-project-selector">所属项目 <select aria-label="晨报所属项目" value={projectId} disabled={busy} onChange={event => { setChosenProject(event.target.value); setDraft(null); setSelected(""); setDetail(null); setNotice("") }}>{!projects.length && <option value="">请先创建项目</option>}{projects.map(project => <option key={project.id} value={project.id}>{projectLabel(projects, project.id)}{data?.configs.some(config => config.project_id === project.id) ? " · 已配置" : ""}</option>)}</select></label>
    <p>每份晨报只属于一个项目。检索依据该项目的目的、README、研究目标与子项目结构；子项目可以另行配置独立晨报。</p>
    <p className="briefing-reading-guide">先看今日导读，再读论文介绍：完整名称、作者、机构、原论文图示、解决的问题、具体方法与实验结果，最后给出阅读判断。邮件使用 HTML 排版，并保留纯文本备用版。</p>
    {(error || data?.config_error) && <p role="alert">{error || data?.config_error}</p>}
    {notice && <p role="status">{notice}</p>}
    {draft && <form className="briefing-settings" onSubmit={event => { event.preventDefault(); const fields = new FormData(event.currentTarget); const config = { ...draft, categories: split(String(fields.get("categories") ?? "")), keywords: split(String(fields.get("keywords") ?? "")) }; void action(async () => { await api.saveBriefingConfig(config); setDraft(null) }, "设置已保存，无需重启。") }}>
      <label><input type="checkbox" checked={draft.enabled} onChange={e => update({ enabled: e.target.checked })}/>开启每日生成</label>
      <label>北京时间<input type="time" required value={draft.time} onChange={e => update({ time: e.target.value })}/></label>
      <label>arXiv 分类（逗号分隔）<input name="categories" required defaultValue={draft.categories.join(", ")}/></label>
      <label>补充关注词（可选，逗号分隔）<textarea name="keywords" defaultValue={draft.keywords.join(", ")}/></label>
      <label>最多候选论文数<input type="number" min="1" max="20" value={draft.max_papers} onChange={e => update({ max_papers: Number(e.target.value) })}/></label>
      <label>深读候选数<input type="number" min="0" max={Math.min(5, draft.max_papers)} value={draft.fulltext_papers} onChange={e => update({ fulltext_papers: Number(e.target.value) })}/></label>
      <p>候选数是筛选上限，不是必写篇数。正文精选最多 3 篇重点，其余只保留少量短讯；深读会优先提供方法、实验和结论节选，证据不足会明确说明。</p>
      <p>所属项目：{projectLabel(projects, draft.project_id)}。无需重复填写项目已有的研究方向；项目资料变化后会重新规划检索词。</p>
      <label><input type="checkbox" checked={draft.email_enabled} disabled={!data?.mail_configured} onChange={e => update({ email_enabled: e.target.checked })}/>邮件投递{!data?.mail_configured && "（服务端尚未设置凭据文件）"}</label>
      <label>收件邮箱<input type="email" required={draft.email_enabled} value={draft.recipient} onChange={e => update({ recipient: e.target.value })}/></label>
      <p>无需保持网页开启。启用邮件后，无新增也会发送状态简报；最终检索或生成失败会发送异常通知，不静默跳过。邮件失败不会重新生成晨报。SMTP 密码仅从服务器本地文件读取。</p>
      <button disabled={busy} type="submit">保存设置</button>
    </form>}
    {item ? <><div className="briefing-actions"><select aria-label="晨报日期" value={item.id} onChange={e => setSelected(e.target.value)}>{items.map(entry => <option key={entry.id} value={entry.id}>{entry.day} · {labels[entry.status] ?? entry.status}</option>)}</select><span>生成：{labels[item.status] ?? item.status} · 邮件：{labels[item.mail_status] ?? item.mail_status}</span>
      {canNotifyBriefing(item) && !["sent", "sending"].includes(item.mail_status) && <button disabled={busy || !config?.email_enabled || item.mail_attempts >= 3} onClick={() => { if (item.mail_status === "uncertain" && !window.confirm("上次邮件可能已送达。确认检查邮箱后仍要重发？")) return; void action(() => api.sendBriefing(item.id), "已提交发送请求，请查看发送状态。") }}>发送{item.status === "completed" ? "邮件" : "状态通知"}</button>}</div>
      {item.error && <p role="alert">{item.error}</p>}{item.mail_error && <p role="alert">{item.mail_error}</p>}
      {item.status === "failed" && !canNotifyBriefing(item) && <p role="status">已安排自动重试；重试结束仍失败会发送异常通知，不会记为“无新增”。</p>}
      {item.status === "running" && <p role="status">正在检索和整理论文，可离开页面，完成后会保存在这里。</p>}
      {detail?.id === item.id && detail.search_plan && <details><summary>本期项目检索方向</summary><p>{detail.search_plan.rationale}</p>{detail.search_plan.topics?.length?detail.search_plan.topics.map(topic=><div key={topic.id}><h3>{topic.label}</h3><p>{topic.intent}</p><p>{topic.project_ids.map(id=>projectLabel(projects,id)).join(" · ")}</p><p>{topic.terms.join(" / ")}</p></div>):<><p>{detail.search_plan.terms.join(" · ")}</p><code className="briefing-query">{detail.search_plan.query}</code></>}</details>}
      {detail?.id === item.id && detail.search_diagnostics && <BriefingSearchDiagnostics diagnostics={detail.search_diagnostics}/>}
      {detail?.id === item.id && item.status === "empty" && !detail.search_diagnostics && <p role="alert">这是旧版留下的空结果，未保存检索明细，无法核验是否完整检索；尚未投递时可点击“重新检索今日晨报”。</p>}
      {detail?.id === item.id && canNotifyBriefing(item) && detail.email_html && <details><summary>HTML 邮件预览（与发送模板一致，不发送邮件）</summary><BriefingEmailPreview html={detail.email_html}/></details>}
      {detail?.id === item.id && detail.markdown && <details open><summary>{item.day} 晨报正文</summary><div className="briefing-content chat-markdown"><ChatMarkdown>{detail.markdown}</ChatMarkdown></div></details>}
      {detail?.id === item.id && detail.papers && item.status === "completed" && <BriefingPaperPicker key={item.id} briefingId={item.id} ownerId={item.project_id} papers={detail.papers} projects={projects}/>}
      {item.conversation_id && <p>可在 Codex 历史对话中打开「{item.day} 论文晨报」继续追问。</p>}
    </> : <p>这个项目还没有晨报。保存项目晨报设置后，可手动生成第一份。</p>}
  </section>
}

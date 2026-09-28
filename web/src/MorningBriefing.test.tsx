import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { BriefingEmailPreview, BriefingSearchDiagnostics, MorningBriefing, canNotifyBriefing, newBriefingConfig } from "./MorningBriefing"
import type { Briefing } from "./MorningBriefing"
import { BriefingPaperPicker, projectLabel } from "./BriefingPaperPicker"
import type { Project } from "./types"

describe("MorningBriefing", () => {
  it("explains dynamic themes, editorial omissions and unresolved scope independently",()=>{
    const html=renderToStaticMarkup(<BriefingSearchDiagnostics diagnostics={{stage:"completed",coverage:[{id:"shared",label:"跨层级稳健控制",candidates:7,selected:2,status:"selected"},{id:"compute",label:"计算预算",candidates:4,selected:0,status:"budget_limited"}],publication_coverage:[{id:"shared",published:1},{id:"compute",published:0}],dispositions:[{project_id:"ambiguous",kind:"clarification",reason:"缩写需要定义"}]}}/> )
    expect(html).toContain("跨层级稳健控制")
    expect(html).toContain("计算预算")
    expect(html).toContain("正文提及 1 篇")
    expect(html).toContain("阅读预算未选入，不等于没有新增")
    expect(html).toContain("缩写需要定义")
  })
  it("shows a timeout and fallback separately from empty search results",()=>{
    const html=renderToStaticMarkup(<BriefingSearchDiagnostics diagnostics={{stage:"fallback_retrieval",primary:{received:0,within_window:0,before_window:0,pages:0,complete:false,limit_reached:false,latest_updated:null,error_kind:"timeout",last_request_ms:60001}}}/> )
    expect(html).toContain("主题请求失败，改用分类检索")
    expect(html).toContain("请求超时")
    expect(html).toContain("60.0")
    expect(html).not.toContain("复查后无新增")
  })
  it("shows retrieval counts and incomplete coverage instead of pretending there are no papers", () => {
    const html = renderToStaticMarkup(<BriefingSearchDiagnostics diagnostics={{stage: "failed", primary: {received: 200, within_window: 2, before_window: 198, pages: 1, complete: true, limit_reached: false, latest_updated: null}, primary_selection: {retrieved: 2, unseen: 2, candidates: 0}, fallback: {received: 2000, within_window: 2000, before_window: 0, pages: 10, complete: false, limit_reached: true, latest_updated: null}}}/> )
    expect(html).toContain("未完成，不能视作无新增")
    expect(html).toContain("去重后 2 篇")
    expect(html).toContain("达到上限，未完整覆盖")
  })
  it("permits empty and final failure notices without mailing every retry", () => {
    expect(canNotifyBriefing({status: "empty"} as Briefing)).toBe(true)
    expect(canNotifyBriefing({status: "failed", attempts: 1, next_attempt_at: "2099-01-01"} as Briefing)).toBe(false)
    expect(canNotifyBriefing({status: "failed", attempts: 3, next_attempt_at: "2099-01-01"} as Briefing)).toBe(true)
    expect(canNotifyBriefing({status: "failed", attempts: 1, next_attempt_at: null} as Briefing)).toBe(true)
    expect(canNotifyBriefing({status: "running"} as Briefing)).toBe(false)
  })
  const projects: Project[] = [{ id: "ei", name: "EI", parent_id: null }, { id: "vla", name: "VLA", parent_id: "ei" }].map(project => ({ ...project, slug: project.id, purpose: "", created_at: "", updated_at: "" }))
  it("requires an explicit project without enabling schedules or mail implicitly", () => {
    expect(newBriefingConfig("vla")).toMatchObject({ project_id: "vla", enabled: false, email_enabled: false, keywords: [] })
    expect(projectLabel(projects, "vla")).toBe("EI / VLA")
    const html = renderToStaticMarkup(<MorningBriefing projects={projects}/> )
    expect(html).toContain("晨报所属项目")
    expect(html).toContain("EI / VLA")
    expect(html).not.toContain("全局兴趣")
  })
  it("offers explicit selection and project paths without automatic import", () => {
    const html = renderToStaticMarkup(<BriefingPaperPicker briefingId="briefing" ownerId="ei" projects={projects} papers={[{ key: "work", title: "Paper title", authors: ["Author"], source_url: "https://arxiv.org/abs/1234.56789", year: 2026, paper_id: "local", project_ids: ["ei"] }]}/> )
    expect(html).toContain("EI / VLA")
    expect(html).toContain("Paper title")
    expect(html).toContain("已加入")
    expect(html).toContain("加入所选论文（0）")
  })
  it("renders an explicit disabled empty state without credential fields", () => {
    const html = renderToStaticMarkup(<MorningBriefing projects={[]}/> )
    expect(html).toContain("论文晨报")
    expect(html).toContain("<h1>论文晨报</h1>")
    expect(html).toContain("完整名称、作者")
    expect(html).toContain("HTML 排版")
    expect(html).toContain("定时生成未开启")
    expect(html).toContain("还没有晨报")
    expect(html).not.toContain('type="password"')
    expect(html).not.toContain("PASSWD")
  })
  it("previews server-rendered email in a sandbox without script privileges", () => {
    const html = renderToStaticMarkup(<BriefingEmailPreview html={'<h1>论文标题</h1><p>作者：A</p>'}/>)
    expect(html).toContain('title="HTML 邮件预览"')
    expect(html).toContain('srcDoc="&lt;h1&gt;论文标题')
    expect(html).toContain('sandbox="allow-popups allow-popups-to-escape-sandbox"')
    expect(html).not.toContain('allow-scripts')
    expect(html).not.toContain('allow-same-origin')
  })
})

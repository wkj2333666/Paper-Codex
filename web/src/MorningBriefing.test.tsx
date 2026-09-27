import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { BriefingEmailPreview, MorningBriefing, newBriefingConfig } from "./MorningBriefing"
import { BriefingPaperPicker, projectLabel } from "./BriefingPaperPicker"
import type { Project } from "./types"

describe("MorningBriefing", () => {
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

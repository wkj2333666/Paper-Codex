import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { BriefingEmailPreview, MorningBriefing } from "./MorningBriefing"

describe("MorningBriefing", () => {
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

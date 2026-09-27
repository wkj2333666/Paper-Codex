import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { MorningBriefing } from "./MorningBriefing"

describe("MorningBriefing", () => {
  it("renders an explicit disabled empty state without credential fields", () => {
    const html = renderToStaticMarkup(<MorningBriefing projects={[]}/> )
    expect(html).toContain("论文晨报")
    expect(html).toContain("<h1>论文晨报</h1>")
    expect(html).toContain("定时生成未开启")
    expect(html).toContain("还没有晨报")
    expect(html).not.toContain('type="password"')
    expect(html).not.toContain("PASSWD")
  })
})

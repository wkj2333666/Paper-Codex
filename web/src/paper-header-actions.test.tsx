import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { PaperHeaderActions } from "./PaperHeaderActions"

const callbacks = {
  onOpenPdf: () => {},
  onReanalyze: () => {},
  onTrash: () => {},
  onToggleGraph: () => {},
}

describe("PaperHeaderActions", () => {
  it("puts related knowledge beside the three existing paper actions", () => {
    const html = renderToStaticMarkup(
      <PaperHeaderActions reanalyzing={false} paperGraphOpen={false} {...callbacks}/>,
    )

    expect(html).toContain("阅读原文")
    expect(html).toContain("重新分析")
    expect(html).toContain("移入回收站")
    expect(html).toContain("相关知识")
    expect(html).toContain('aria-controls="paper-graph-panel"')
    expect(html).toContain('aria-expanded="false"')
  })

  it("exposes the same action as expanded while the graph is open", () => {
    const html = renderToStaticMarkup(
      <PaperHeaderActions reanalyzing paperGraphOpen {...callbacks}/>,
    )

    expect(html).toContain('aria-expanded="true"')
  })
})

import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { PaperHeaderActions } from "./PaperHeaderActions"

const callbacks = {
  sourceUrl: "https://arxiv.org/abs/2606.03784",
  analysisAction: <button>重新分析</button>,
  onTrash: () => {},
  onToggleGraph: () => {},
}

describe("PaperHeaderActions", () => {
  it("puts related knowledge beside the three existing paper actions", () => {
    const html = renderToStaticMarkup(
      <PaperHeaderActions paperGraphOpen={false} {...callbacks}/>,
    )

    expect(html).toContain("打开原始链接")
    expect(html).toContain('href="https://arxiv.org/abs/2606.03784"')
    expect(html).toContain('target="_blank"')
    expect(html).toContain('rel="noopener noreferrer"')
    expect(html).toContain("重新分析")
    expect(html).toContain("移入回收站")
    expect(html).toContain("相关知识")
    expect(html).toContain('aria-controls="paper-graph-panel"')
    expect(html).toContain('aria-expanded="false"')
  })

  it("exposes the same action as expanded while the graph is open", () => {
    const html = renderToStaticMarkup(
      <PaperHeaderActions paperGraphOpen {...callbacks}/>,
    )

    expect(html).toContain('aria-expanded="true"')
  })
  it("does not invent a link for uploaded papers", () => {
    const html = renderToStaticMarkup(<PaperHeaderActions {...callbacks} sourceUrl={null} paperGraphOpen={false}/> )
    expect(html).toContain("无原始链接")
    expect(html).not.toContain('target="_blank"')
  })
})

import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { MobilePanelRails, PanelCollapseButton, PanelRail } from "./PanelControls"

describe("panel controls", () => {
  it("labels collapsed and expanded controls for assistive technology", () => {
    const rail = renderToStaticMarkup(
      <PanelRail panel="sidebar" label="文件树" side="left" onExpand={() => {}} />,
    )
    const collapse = renderToStaticMarkup(
      <PanelCollapseButton label="Codex" direction="right" onCollapse={() => {}} />,
    )

    expect(rail).toContain('aria-label="展开文件树"')
    expect(rail).toContain('aria-expanded="false"')
    expect(collapse).toContain('aria-label="收起Codex"')
    expect(collapse).toContain('aria-expanded="true"')
  })

  it("does not duplicate the paper graph action as a narrow-screen rail", () => {
    const html = renderToStaticMarkup(<MobilePanelRails onOpen={() => {}} />)

    expect(html).not.toContain("相关知识")
    expect(html).toContain("Codex")
  })
})

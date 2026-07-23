import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { ResizableDivider, dividerDeltaForKey } from "./ResizableDivider"

describe("ResizableDivider", () => {
  it("exposes the current panel size as an accessible separator", () => {
    const html = renderToStaticMarkup(
      <ResizableDivider
        panel="codex"
        value={380}
        min={320}
        max={640}
        onResize={() => {}}
        onReset={() => {}}
      />,
    )

    expect(html).toContain('role="separator"')
    expect(html).toContain('aria-valuenow="380"')
    expect(html).toContain('aria-valuemin="320"')
    expect(html).toContain('aria-valuemax="640"')
    expect(html).toContain('tabindex="0"')
  })

  it("maps keyboard movement to normal and coarse resize steps", () => {
    expect(dividerDeltaForKey("ArrowLeft", false)).toBe(-10)
    expect(dividerDeltaForKey("ArrowRight", true)).toBe(40)
    expect(dividerDeltaForKey("Enter", false)).toBeNull()
  })
})

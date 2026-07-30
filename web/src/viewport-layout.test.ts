// @ts-expect-error Node built-ins are available in Vitest
import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"

const viewportStyles = readFileSync(new URL("./viewport.css", import.meta.url), "utf8")
const entrypoint = readFileSync(new URL("./main.tsx", import.meta.url), "utf8")
const panelLayout = readFileSync(new URL("./panel-layout.css", import.meta.url), "utf8")
const compactViewportStyles = viewportStyles.replace(/\s+/g, "")

function declarations(source: string, selector: string) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
  const match = source.match(new RegExp(`${escaped}\\s*\\{([^}]*)\\}`))
  expect(match, `missing CSS rule for ${selector}`).not.toBeNull()
  return match?.[1] ?? ""
}

describe("viewport containment", () => {
  it("keeps document scrolling disabled and lets the application inherit the root height", () => {
    const roots = declarations(compactViewportStyles, "html,body,#root")
    const body = declarations(compactViewportStyles, "body")
    const shell = declarations(compactViewportStyles, ".app-shell")

    const viewportImport = entrypoint.indexOf('import "./viewport.css"')
    expect(viewportImport).toBeGreaterThan(entrypoint.indexOf('import "./styles.css"'))
    expect(viewportImport).toBeGreaterThan(entrypoint.indexOf('import "./panel-layout.css"'))
    expect(roots).toMatch(/height:\s*100%/)
    expect(roots).toMatch(/overflow:\s*hidden/)
    expect(body).toMatch(/min-height:\s*0/)
    expect(body).not.toMatch(/100vh/)
    expect(shell).toMatch(/height:\s*100%/)
    expect(shell).toMatch(/min-height:\s*0/)
    expect(shell).not.toMatch(/100vh/)
  })

  it("keeps narrow workspace panels inside the inherited application height", () => {
    const narrowLayout = panelLayout.slice(panelLayout.indexOf("@media(max-width:1050px)"))

    expect(narrowLayout).not.toMatch(/height:\s*100vh/)
    expect(narrowLayout).toMatch(/\.main-pane\s*\{[^}]*height:\s*100%/)
    expect(narrowLayout).toMatch(/\.workspace-panel\s*\{[^}]*height:\s*100%/)
  })
})

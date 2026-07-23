// @ts-expect-error Node built-ins are available in Vitest
import { readFileSync } from "node:fs"
import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { ThemeToggle } from "./ThemeToggle"

const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8")

describe("theme toggle", () => {
  it("describes the current mode and next action accessibly", () => {
    const html = renderToStaticMarkup(<ThemeToggle preference="system" resolvedTheme="dark" onCycle={() => {}} />)
    expect(html).toContain('class="theme-toggle"')
    expect(html).toContain('type="button"')
    expect(html).toContain("跟随系统")
    expect(html).toContain("切换到浅色模式")
    expect(html).toContain("aria-label=")
    expect(html).toContain("title=")
  })

  it("wires the resolved theme to the document and reading surfaces", () => {
    expect(appSource).toContain("document.documentElement.dataset.theme")
    expect(appSource).toContain("document.documentElement.style.colorScheme")
    expect(appSource).toContain("<ThemeToggle")
    expect(appSource).toContain("theme={resolvedTheme}")
  })
})

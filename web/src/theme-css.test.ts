// @ts-expect-error Node built-ins are available in Vitest
import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"

const themeCss = readFileSync(new URL("./theme.css", import.meta.url), "utf8")

describe("dark theme surface overrides", () => {
  it("defines dark tokens and covers the major application surfaces", () => {
    expect(themeCss).toContain('[data-theme="dark"]')
    expect(themeCss).toMatch(/--ink:/)
    expect(themeCss).toMatch(/--paper:/)
    expect(themeCss).toMatch(/--line:/)
    for (const selector of [".app-shell", ".sidebar", ".activity-pane", ".codex-pane", ".paper-card", ".chat-box", ".login-page"]) {
      expect(themeCss).toContain(`[data-theme="dark"] ${selector}`)
    }
    for (const selector of [".codex-task-header", ".codex-user-prompt", ".codex-worklog", ".codex-settings", ".codex-composer"]) {
      expect(themeCss).toContain(`[data-theme="dark"] ${selector}`)
    }
  })

  it("keeps the selected project label readable in dark mode", () => {
    expect(themeCss).toMatch(/\[data-theme="dark"\] \.project-row\.active \.tree-main\s*\{[^}]*color:\s*var\(--green\)/)
  })

  it("keeps paper analysis prose readable in dark mode", () => {
    for (const selector of [".brief-card li", ".analysis-panel li", ".analysis-panel p", ".markdown"]) {
      expect(themeCss).toContain(`[data-theme="dark"] ${selector}`)
    }
  })
})

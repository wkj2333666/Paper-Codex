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
  })
})

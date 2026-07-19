// @ts-expect-error Node built-ins are available in Vitest
import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"

const app = readFileSync(new URL("./App.tsx", import.meta.url), "utf8")

describe("drawer focus containment", () => {
  it("keeps keyboard focus in the active drawer without disabling a nested paper drawer", () => {
    expect(app).toContain('event.key==="Tab"')
    expect(app).toContain('document.addEventListener("focusin"')
    expect(app).toContain("new MutationObserver")
    expect(app).toContain("cancelAnimationFrame(focusFrame)")
    expect(app).not.toMatch(/<main[^>]*\sinert=/)
  })
})

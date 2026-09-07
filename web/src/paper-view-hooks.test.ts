import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"

describe("PaperView hook ordering", () => {
  it("defines every hook before the paper-loading return", () => {
    const source = readFileSync(new URL("./App.tsx", import.meta.url), "utf8")
    const start = source.indexOf("function PaperView(")
    const end = source.indexOf("\nfunction BriefCard", start)
    expect(start).toBeGreaterThan(-1)
    expect(end).toBeGreaterThan(start)

    const body = source.slice(start, end)
    const loadingReturn = body.indexOf("if(!detail)return")
    expect(loadingReturn).toBeGreaterThan(-1)

    const hookPattern = /(?:^|[^A-Za-z])use[A-Z]\w*\s*\(/g
    const hooks = [...body.matchAll(hookPattern)]
    expect(hooks.length).toBeGreaterThan(0)
    expect(hooks.filter(hook => hook.index > loadingReturn)).toEqual([])
  })
})

import { describe, expect, it } from "vitest"
import { cycleThemePreference, readThemePreference, resolveTheme, THEME_STORAGE_KEY, writeThemePreference } from "./theme"

function storage(initial?: string): Storage {
  let value = initial ?? null
  return {
    getItem: () => value,
    setItem: (_key, next) => { value = next },
    removeItem: () => { value = null },
    clear: () => { value = null },
    key: () => null,
    get length() { return value === null ? 0 : 1 },
  }
}

describe("theme preference", () => {
  it("resolves system preference and lets explicit choices win", () => {
    expect(resolveTheme("system", true)).toBe("dark")
    expect(resolveTheme("system", false)).toBe("light")
    expect(resolveTheme("light", true)).toBe("light")
    expect(resolveTheme("dark", false)).toBe("dark")
  })

  it("reads only valid persisted preferences and writes the stable key", () => {
    expect(readThemePreference(storage("dark"))).toBe("dark")
    expect(readThemePreference(storage("invalid"))).toBe("system")
    expect(readThemePreference(storage())).toBe("system")
    const target = storage()
    writeThemePreference("light", target)
    expect(target.getItem(THEME_STORAGE_KEY)).toBe("light")
  })

  it("cycles system, dark, and light modes predictably", () => {
    expect(cycleThemePreference("system")).toBe("dark")
    expect(cycleThemePreference("dark")).toBe("light")
    expect(cycleThemePreference("light")).toBe("system")
  })
})

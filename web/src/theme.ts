export type ThemePreference = "system" | "light" | "dark"
export type ResolvedTheme = "light" | "dark"

export const THEME_STORAGE_KEY = "paper-codex.theme"

const preferences = new Set<ThemePreference>(["system", "light", "dark"])

export function resolveTheme(preference: ThemePreference, systemDark: boolean): ResolvedTheme {
  if (preference === "dark") return "dark"
  if (preference === "light") return "light"
  return systemDark ? "dark" : "light"
}

export function readThemePreference(storage: Storage | undefined = browserStorage()): ThemePreference {
  try {
    const value = storage?.getItem(THEME_STORAGE_KEY)
    return value && preferences.has(value as ThemePreference) ? value as ThemePreference : "system"
  } catch {
    return "system"
  }
}

export function writeThemePreference(preference: ThemePreference, storage: Storage | undefined = browserStorage()): void {
  try { storage?.setItem(THEME_STORAGE_KEY, preference) } catch { /* private browsing */ }
}

export function cycleThemePreference(preference: ThemePreference): ThemePreference {
  return preference === "system" ? "dark" : preference === "dark" ? "light" : "system"
}

function browserStorage(): Storage | undefined {
  return typeof globalThis !== "undefined" && "localStorage" in globalThis ? globalThis.localStorage : undefined
}

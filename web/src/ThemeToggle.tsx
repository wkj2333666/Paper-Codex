import { Monitor, Moon, Sun } from "lucide-react"
import type { ResolvedTheme, ThemePreference } from "./theme"

export function ThemeToggle({ preference, resolvedTheme, onCycle }: {
  preference: ThemePreference
  resolvedTheme: ResolvedTheme
  onCycle: () => void
}) {
  const CurrentIcon = preference === "system" ? Monitor : resolvedTheme === "dark" ? Moon : Sun
  const currentLabel = preference === "system" ? `跟随系统（当前${resolvedTheme === "dark" ? "深色" : "浅色"}）` : resolvedTheme === "dark" ? "深色模式" : "浅色模式"
  const nextLabel = resolvedTheme === "dark" ? "切换到浅色模式" : "切换到深色模式"
  const label = `${currentLabel}，${nextLabel}`
  return <button className="theme-toggle" type="button" aria-label={label} title={label} onClick={onCycle}><CurrentIcon size={14}/><span>{currentLabel}</span></button>
}

import { Blocks, LoaderCircle, Search, Sparkles, Wrench, X } from "lucide-react"
import {
  codexToolOptionId,
  filterCodexToolCatalog,
  toolPickerKeyIntent,
  type CodexToolCatalogItem,
} from "./codex-tool-picker"

interface CodexToolPickerProps {
  open: boolean
  items: CodexToolCatalogItem[]
  query: string
  activeIndex: number
  loading: boolean
  onQuery: (query: string) => void
  onActiveIndex: (index: number) => void
  onSelect: (item: CodexToolCatalogItem) => void
  onClose: () => void
}

export function CodexToolPicker({
  open,
  items,
  query,
  activeIndex,
  loading,
  onQuery,
  onActiveIndex,
  onSelect,
  onClose,
}: CodexToolPickerProps) {
  if (!open) return null
  const filtered = filterCodexToolCatalog(items, query)
  const active = filtered[activeIndex] ?? filtered[0]
  const skills = filtered.filter((item) => item.kind === "skill")
  const tools = filtered.filter((item) => item.kind === "mcp-tool")
  let optionIndex = -1
  const renderOption = (item: CodexToolCatalogItem) => {
    optionIndex += 1
    const index = optionIndex
    return (
      <button
        id={codexToolOptionId(item)}
        key={item.key}
        type="button"
        role="option"
        aria-selected={active?.key === item.key}
        className={active?.key === item.key ? "active" : ""}
        onPointerEnter={() => onActiveIndex(index)}
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => onSelect(item)}
      >
        <span className="codex-tool-option-icon">{item.kind === "skill" ? <Sparkles /> : <Wrench />}</span>
        <span className="codex-tool-option-copy">
          <span>
            <strong>{item.label}</strong>
            <small>{item.kind === "skill" ? "Skill" : item.serverLabel}</small>
          </span>
          <span>{item.description}</span>
          <code>
            {item.kind === "skill"
              ? `$${item.skill.name.split(":").at(-1)}`
              : `${item.preference.server}/${item.preference.tool}`}
          </code>
        </span>
      </button>
    )
  }

  return (
    <section
      className="codex-tool-picker"
      aria-label="Codex 工具选择器"
      onKeyDown={(event) => {
        const intent = toolPickerKeyIntent(event.key, event.shiftKey)
        if (intent === "ignore") return
        event.preventDefault()
        if (intent === "close") return onClose()
        if (!filtered.length) return
        if (intent === "next") return onActiveIndex((activeIndex + 1) % filtered.length)
        if (intent === "previous") return onActiveIndex((activeIndex - 1 + filtered.length) % filtered.length)
        if (intent === "select") onSelect(active ?? filtered[0])
      }}
    >
      <header>
        <div>
          <Blocks />
          <strong>Skills 与工具</strong>
        </div>
        <button type="button" aria-label="关闭工具选择器" onClick={onClose}>
          <X />
        </button>
      </header>
      <label className="codex-tool-search">
        <Search />
        <input
          value={query}
          onChange={(event) => onQuery(event.target.value)}
          placeholder="搜索 Skills 和工具"
          aria-label="搜索 Skills 和工具"
          autoComplete="off"
        />
      </label>
      <div
        className="codex-tool-options"
        role="listbox"
        aria-label="可用 Skills 和工具"
        aria-activedescendant={active ? codexToolOptionId(active) : undefined}
      >
        {loading && !items.length ? (
          <div className="codex-tool-picker-state">
            <LoaderCircle className="spin" />
            <span>正在读取 Codex 能力…</span>
          </div>
        ) : filtered.length ? (
          <>
            {skills.length > 0 && (
              <section>
                <h4>Skills</h4>
                {skills.map(renderOption)}
              </section>
            )}
            {tools.length > 0 && (
              <section>
                <h4>MCP 工具</h4>
                {tools.map(renderOption)}
              </section>
            )}
          </>
        ) : (
          <div className="codex-tool-picker-state">没有匹配的 Skill 或工具</div>
        )}
      </div>
    </section>
  )
}

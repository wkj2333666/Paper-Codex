import { useEffect, useMemo, useRef, useState } from "react"
import type { FormEvent } from "react"
import { Gauge, LoaderCircle, Minimize2, Search, Send, Settings2, Sparkles, Square, Target, Wrench, X } from "lucide-react"
import { CodexToolPicker } from "./CodexToolPicker"
import { applyCodexCommand, codexCommandCompletion, type CodexCommandCompletion, type CodexCommandDefinition } from "./codex-commands"
import {
  applyCompletion,
  buildCodexToolCatalog,
  completionAtCursor,
  filterCodexToolCatalog,
  toolPickerKeyIntent,
  type CodexToolCatalogItem,
  type CompletionRange,
} from "./codex-tool-picker"
import type { CodexCapabilities, CodexIntegrations, CodexRunSettings, CodexSkill, CodexToolPreference, ResearchMode } from "./types"

export type ComposerKeyIntent = "submit" | "newline" | "ignore"

export function composerKeyIntent({
  key,
  shiftKey,
  isComposing,
  canSubmit,
}: {
  key: string
  shiftKey: boolean
  isComposing: boolean
  canSubmit: boolean
}): ComposerKeyIntent {
  if (key !== "Enter") return "ignore"
  if (shiftKey) return "newline"
  if (isComposing || !canSubmit) return "ignore"
  return "submit"
}

export function settingsTargetIsOutside(
  container: { contains: (target: Node | null) => boolean } | null,
  target: EventTarget | null,
): boolean {
  return !container || !target || !container.contains(target as Node)
}

export function normalizeCodexSettings(
  capabilities: CodexCapabilities,
  settings?: CodexRunSettings | null,
): CodexRunSettings {
  const requestedModel = settings?.model ?? capabilities.default.model
  const model = capabilities.models.find((item) => item.id === requestedModel) ?? capabilities.models[0]
  if (!model) return capabilities.default
  const requestedReasoningEffort = settings?.reasoning_effort ?? capabilities.default.reasoning_effort
  const reasoningEffort = model.supported_reasoning_efforts.includes(requestedReasoningEffort)
    ? requestedReasoningEffort
    : model.default_reasoning_effort
  return {
    model: model.id,
    reasoning_effort: reasoningEffort,
    service_tier: settings?.service_tier === "priority" && model.supports_fast ? "priority" : null,
  }
}

const effortLabels: Record<string, string> = {
  minimal: "最少",
  low: "低",
  medium: "中",
  high: "高",
  xhigh: "极高",
  max: "最大",
}

const effortLabel = (effort: string) => effortLabels[effort] ?? effort

interface CodexComposerProps {
  text: string
  placeholder: string
  busy: boolean
  answerRunning: boolean
  projectResearchScope: boolean
  controlledResearchAvailable: boolean
  researchMode: ResearchMode
  capabilities: CodexCapabilities
  integrations: CodexIntegrations | null
  integrationsLoading: boolean
  settings: CodexRunSettings
  selectedSkill: CodexSkill | null
  selectedTools: CodexToolPreference[]
  onSelectSkill: (skill: CodexSkill) => void
  onClearSkill: () => void
  onToggleTool: (preference: CodexToolPreference) => void
  onRequestIntegrations: () => void
  onText: (value: string) => void
  onSubmit: (event: FormEvent) => void
  onCancel: () => void
  onResearchMode: (mode: ResearchMode) => void
  onSettings: (settings: CodexRunSettings) => void
}

export function CodexComposer({
  text,
  placeholder,
  busy,
  answerRunning,
  projectResearchScope,
  controlledResearchAvailable,
  researchMode,
  capabilities,
  integrations,
  integrationsLoading,
  settings,
  selectedSkill,
  selectedTools,
  onSelectSkill,
  onClearSkill,
  onToggleTool,
  onRequestIntegrations,
  onText,
  onSubmit,
  onCancel,
  onResearchMode,
  onSettings,
}: CodexComposerProps) {
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [toolPickerOpen, setToolPickerOpen] = useState(false)
  const [toolPickerMode, setToolPickerMode] = useState<"button" | "completion">("button")
  const [toolQuery, setToolQuery] = useState("")
  const [toolActiveIndex, setToolActiveIndex] = useState(0)
  const [commandCompletion, setCommandCompletion] = useState<CodexCommandCompletion | null>(null)
  const [commandActiveIndex, setCommandActiveIndex] = useState(0)
  const settingsRef = useRef<HTMLDetailsElement | null>(null)
  const settingsSummaryRef = useRef<HTMLElement | null>(null)
  const formRef = useRef<HTMLFormElement | null>(null)
  const textareaRef = useRef<HTMLTextAreaElement | null>(null)
  const completionRef = useRef<CompletionRange | null>(null)
  const selectedModel = capabilities.models.find((item) => item.id === settings.model) ?? capabilities.models[0]
  const speed = settings.service_tier === "priority" ? "快速" : "标准"
  const toolCatalog = useMemo(() => buildCodexToolCatalog(integrations), [integrations])
  const visibleTools = useMemo(
    () => filterCodexToolCatalog(toolCatalog, toolQuery),
    [toolCatalog, toolQuery],
  )

  const closeToolPicker = () => {
    setToolPickerOpen(false)
    completionRef.current = null
  }
  const syncCommandCompletion = (value:string,cursor:number) => {
    const completion=codexCommandCompletion(value,cursor)
    setCommandCompletion(completion)
    setCommandActiveIndex(0)
    if(completion)closeToolPicker()
    return completion
  }
  const requestToolCatalog = () => {
    onRequestIntegrations()
    setSettingsOpen(false)
  }
  const openToolPicker = () => {
    requestToolCatalog()
    completionRef.current = null
    setToolPickerMode("button")
    setToolQuery("")
    setToolActiveIndex(0)
    setToolPickerOpen((open) => !open || toolPickerMode !== "button")
  }
  const syncCompletion = (value: string, cursor: number) => {
    const range = completionAtCursor(value, cursor)
    completionRef.current = range
    if (!range) {
      if (toolPickerMode === "completion") setToolPickerOpen(false)
      return
    }
    requestToolCatalog()
    setToolPickerMode("completion")
    setToolQuery(range.query)
    setToolActiveIndex(0)
    setToolPickerOpen(true)
  }
  const restoreTextarea = (cursor?: number) => {
    requestAnimationFrame(() => {
      textareaRef.current?.focus()
      if (cursor !== undefined) textareaRef.current?.setSelectionRange(cursor, cursor)
    })
  }
  const chooseTool = (item: CodexToolCatalogItem) => {
    let nextCursor: number | undefined
    const range = completionRef.current
    if (item.kind === "skill") {
      onSelectSkill(item.skill)
      if (range) {
        const leaf = item.skill.name.split(":").at(-1) ?? item.skill.name
        const mention = `$${leaf}${range.end === text.length ? " " : ""}`
        const next = applyCompletion(text, range, mention)
        onText(next.text)
        nextCursor = next.cursor
      }
    } else {
      onToggleTool(item.preference)
      if (range) {
        const next = applyCompletion(text, range, "")
        onText(next.text)
        nextCursor = next.cursor
      }
    }
    closeToolPicker()
    restoreTextarea(nextCursor)
  }
  const chooseCommand=(command:CodexCommandDefinition)=>{
    if(!commandCompletion)return
    const next=applyCodexCommand(text,commandCompletion,command.name)
    onText(next.text)
    setCommandCompletion(null)
    restoreTextarea(next.cursor)
  }

  useEffect(() => {
    if (!settingsOpen) return
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (settingsTargetIsOutside(settingsRef.current, event.target)) setSettingsOpen(false)
    }
    document.addEventListener("pointerdown", closeOnOutsidePointer)
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer)
  }, [settingsOpen])

  useEffect(() => {
    if (!toolPickerOpen) return
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (settingsTargetIsOutside(formRef.current, event.target)) closeToolPicker()
    }
    document.addEventListener("pointerdown", closeOnOutsidePointer)
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer)
  }, [toolPickerOpen])

  useEffect(() => {
    if (toolActiveIndex < visibleTools.length) return
    setToolActiveIndex(Math.max(0, visibleTools.length - 1))
  }, [toolActiveIndex, visibleTools.length])

  return (
    <form
      ref={formRef}
      className={`conversation-composer codex-composer${researchMode === "explicit" ? " research-explicit" : ""}`}
      onSubmit={onSubmit}
      onBlur={(event) => {
        if (toolPickerOpen && settingsTargetIsOutside(event.currentTarget, event.relatedTarget))
          closeToolPicker()
        if(commandCompletion&&settingsTargetIsOutside(event.currentTarget,event.relatedTarget))setCommandCompletion(null)
      }}
    >
      {commandCompletion&&<div className="codex-command-picker" role="listbox" aria-label="Codex 命令">
        <header><strong>命令</strong><span>原生 Codex 工作流</span></header>
        {commandCompletion.items.length?commandCompletion.items.map((command,index)=><button type="button" role="option" aria-selected={index===commandActiveIndex} className={index===commandActiveIndex?"active":""} key={command.name} onMouseDown={event=>event.preventDefault()} onClick={()=>chooseCommand(command)}>{command.name==="goal"?<Target/>:<Minimize2/>}<span><strong>/{command.name}</strong><small>{command.label} · {command.description}</small></span></button>):<p>没有匹配的命令</p>}
      </div>}
      <CodexToolPicker
        open={toolPickerOpen}
        items={toolCatalog}
        query={toolQuery}
        activeIndex={toolActiveIndex}
        loading={integrationsLoading}
        onQuery={(query) => {
          completionRef.current = null
          setToolPickerMode("button")
          setToolQuery(query)
          setToolActiveIndex(0)
        }}
        onActiveIndex={setToolActiveIndex}
        onSelect={chooseTool}
        onClose={() => {
          closeToolPicker()
          restoreTextarea()
        }}
      />
      {(selectedSkill || selectedTools.length > 0) && (
        <div className="codex-tool-chips" aria-label="本轮选择的 Skills 和工具">
          {selectedSkill && (
            <div className="codex-selected-skill" aria-label={`已选择 Skill：${selectedSkill.display_name}`}>
              <Sparkles />
              <span>
                <small>Skill</small>
                <strong>{selectedSkill.display_name}</strong>
              </span>
              <button type="button" aria-label="取消选择 Skill" onClick={onClearSkill}>
                <X />
              </button>
            </div>
          )}
          {selectedTools.map((preference) => (
            <div
              className="codex-selected-tool"
              key={`${preference.server}:${preference.tool}`}
              aria-label={`已选择工具：${preference.server}/${preference.tool}`}
            >
              <Wrench />
              <span>
                <small>{preference.server}</small>
                <strong>{preference.tool}</strong>
              </span>
              <button
                type="button"
                aria-label={`取消选择工具 ${preference.server}/${preference.tool}`}
                onClick={() => onToggleTool(preference)}
              >
                <X />
              </button>
            </div>
          ))}
        </div>
      )}
      <textarea
        ref={textareaRef}
        value={text}
        onChange={(event) => {
          onText(event.target.value)
          if(!syncCommandCompletion(event.target.value,event.target.selectionStart))syncCompletion(event.target.value, event.target.selectionStart)
        }}
        onClick={(event) => {if(!syncCommandCompletion(event.currentTarget.value,event.currentTarget.selectionStart))syncCompletion(event.currentTarget.value, event.currentTarget.selectionStart)}}
        onKeyUp={(event) => {
          if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key))
            syncCompletion(event.currentTarget.value, event.currentTarget.selectionStart)
        }}
        onKeyDown={(event) => {
          if(commandCompletion){
            if(event.key==="Escape"){event.preventDefault();setCommandCompletion(null);return}
            if(["ArrowDown","ArrowUp","Enter","Tab"].includes(event.key)){
              event.preventDefault()
              if(!commandCompletion.items.length)return
              if(event.key==="ArrowDown"){setCommandActiveIndex((commandActiveIndex+1)%commandCompletion.items.length);return}
              if(event.key==="ArrowUp"){setCommandActiveIndex((commandActiveIndex-1+commandCompletion.items.length)%commandCompletion.items.length);return}
              chooseCommand(commandCompletion.items[commandActiveIndex]??commandCompletion.items[0]);return
            }
          }
          if (toolPickerOpen) {
            const pickerIntent = toolPickerKeyIntent(event.key, event.shiftKey)
            if (pickerIntent !== "ignore") {
              event.preventDefault()
              if (pickerIntent === "close") {
                closeToolPicker()
                return
              }
              if (!visibleTools.length) return
              if (pickerIntent === "next") {
                setToolActiveIndex((toolActiveIndex + 1) % visibleTools.length)
                return
              }
              if (pickerIntent === "previous") {
                setToolActiveIndex((toolActiveIndex - 1 + visibleTools.length) % visibleTools.length)
                return
              }
              chooseTool(visibleTools[toolActiveIndex] ?? visibleTools[0])
              return
            }
          }
          const intent = composerKeyIntent({
            key: event.key,
            shiftKey: event.shiftKey,
            isComposing: event.nativeEvent.isComposing,
            canSubmit: !busy && !answerRunning && Boolean(text.trim()),
          })
          if (intent === "submit") {
            event.preventDefault()
            event.currentTarget.form?.requestSubmit()
          }
        }}
        placeholder={placeholder}
      />
      <div className="codex-composer-context">
        <div className="codex-context-controls">
          <button
            type="button"
            className="codex-tool-toggle"
            aria-label="选择 Skills 和工具"
            aria-expanded={toolPickerOpen}
            onClick={openToolPicker}
          >
            <Wrench />
            <span>工具</span>
          </button>
          <details
            ref={settingsRef}
            className="codex-settings-popover"
            open={settingsOpen}
            onToggle={(event) => {
              setSettingsOpen(event.currentTarget.open)
              if (event.currentTarget.open) closeToolPicker()
            }}
            onBlur={(event) => {
              if (settingsTargetIsOutside(event.currentTarget, event.relatedTarget)) setSettingsOpen(false)
            }}
            onKeyDown={(event) => {
              if (event.key !== "Escape") return
              event.preventDefault()
              setSettingsOpen(false)
              settingsSummaryRef.current?.focus()
            }}
          >
            <summary ref={settingsSummaryRef} aria-label="Codex 运行设置" title="Codex 运行设置">
              <Settings2 />
              <span>{selectedModel?.display_name ?? settings.model}</span>
            </summary>
            <div className="codex-settings" role="dialog" aria-label="Codex 运行设置">
              <div className="codex-settings-heading">
                <Gauge />
                <div>
                  <strong>本轮运行设置</strong>
                  <span>设置会保存在当前对话中</span>
                </div>
              </div>
              <label>
                模型
                <select
                  value={settings.model}
                  onChange={(event) => {
                    const model = capabilities.models.find((item) => item.id === event.target.value) ?? selectedModel
                    if (model)
                      onSettings(
                        normalizeCodexSettings(capabilities, {
                          ...settings,
                          model: model.id,
                          reasoning_effort: model.default_reasoning_effort,
                          service_tier: null,
                        }),
                      )
                  }}
                >
                  {capabilities.models.map((model) => (
                    <option key={model.id} value={model.id}>
                      {model.display_name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                推理强度
                <select
                  value={settings.reasoning_effort}
                  onChange={(event) => onSettings({ ...settings, reasoning_effort: event.target.value })}
                >
                  {(selectedModel?.supported_reasoning_efforts ?? []).map((effort) => (
                    <option key={effort} value={effort}>
                      {effortLabel(effort)}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                速度
                <select
                  value={settings.service_tier === "priority" ? "fast" : "standard"}
                  disabled={!selectedModel?.supports_fast}
                  onChange={(event) =>
                    onSettings({ ...settings, service_tier: event.target.value === "fast" ? "priority" : null })
                  }
                >
                  <option value="standard">标准</option>
                  <option value="fast">快速</option>
                </select>
              </label>
            </div>
          </details>
          <span className="codex-context-chip">{effortLabel(settings.reasoning_effort)}推理</span>
          <span className="codex-context-chip">{speed}</span>
          {projectResearchScope &&
            (controlledResearchAvailable ? (
              <button
                type="button"
                className="codex-research-toggle"
                aria-label="检索论文"
                aria-pressed={researchMode === "explicit"}
                onClick={() => onResearchMode(researchMode === "explicit" ? "auto" : "explicit")}
              >
                <Search />
                <span>{researchMode === "explicit" ? "将检索论文" : "论文检索"}</span>
              </button>
            ) : (
              <span className="codex-research-unavailable">当前 Codex 版本不支持受控论文检索</span>
            ))}
        </div>
        {answerRunning ? (
          <button type="button" className="codex-send codex-stop" aria-label="停止回答" onClick={onCancel}>
            <Square />
          </button>
        ) : (
          <button className="codex-send" aria-label="发送消息" disabled={busy || !text.trim()}>
            {busy ? <LoaderCircle className="spin" /> : <Send />}
          </button>
        )}
      </div>
    </form>
  )
}

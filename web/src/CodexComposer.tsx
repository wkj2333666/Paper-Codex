import type { FormEvent } from "react"
import { Gauge, LoaderCircle, Search, Send, Settings2, Square } from "lucide-react"
import type { CodexCapabilities, CodexRunSettings, ResearchMode } from "./types"

export function normalizeCodexSettings(
  capabilities: CodexCapabilities,
  settings?: CodexRunSettings | null,
): CodexRunSettings {
  const model = capabilities.models.find((item) => item.id === settings?.model) ?? capabilities.models[0]
  if (!model) return capabilities.default
  const reasoningEffort = model.supported_reasoning_efforts.includes(settings?.reasoning_effort ?? "")
    ? settings!.reasoning_effort
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
  settings: CodexRunSettings
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
  settings,
  onText,
  onSubmit,
  onCancel,
  onResearchMode,
  onSettings,
}: CodexComposerProps) {
  const selectedModel = capabilities.models.find((item) => item.id === settings.model) ?? capabilities.models[0]
  const speed = settings.service_tier === "priority" ? "快速" : "标准"
  return (
    <form
      className={`conversation-composer codex-composer${researchMode === "explicit" ? " research-explicit" : ""}`}
      onSubmit={onSubmit}
    >
      <textarea value={text} onChange={(event) => onText(event.target.value)} placeholder={placeholder} />
      <div className="codex-composer-context">
        <div className="codex-context-controls">
          <details className="codex-settings-popover">
            <summary aria-label="Codex 运行设置" title="Codex 运行设置">
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

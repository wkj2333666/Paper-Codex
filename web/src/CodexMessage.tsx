import { Bot, CircleAlert, LoaderCircle } from "lucide-react"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"
import type { ChatMessage, MessageCitation } from "./types"

const researchLabels: Partial<Record<NonNullable<ChatMessage["progress_phase"]>, string>> = {
  "research-planning": "正在生成检索式…",
  "research-searching": "正在检索外部论文…",
  "research-deduplicating": "正在规范化与去重…",
  "research-inspecting-abstract": "正在核验摘要…",
  "research-fetching-fulltext": "正在获取开放全文…",
  "research-saving-candidates": "正在保存候选论文…",
  "research-partial": "部分来源不可用，正在使用其余结果…",
}

export function ConversationProgress({
  phase,
  label,
}: {
  phase: ChatMessage["progress_phase"]
  label?: string
}) {
  const fallback =
    (phase && researchLabels[phase]) ??
    (phase === "reading"
      ? "Codex 正在读取论文…"
      : phase === "tool"
        ? "Codex 正在调用研究工具…"
        : phase === "answering"
          ? "Codex 正在生成回答…"
          : "Codex 正在分析证据并组织回答…")
  return (
    <div className="conversation-progress" role="status">
      <span className="codex-progress-dot">
        <LoaderCircle className="spin" />
      </span>
      <span className="codex-progress-copy">
        <strong>工作过程</strong>
        <span>{label || fallback}</span>
      </span>
    </div>
  )
}

export function CodexMessage({
  message,
  onCitation,
}: {
  message: ChatMessage
  onCitation: (citation: MessageCitation) => void
}) {
  if (message.role === "user") {
    return (
      <article className="codex-turn codex-user-message">
        <span className="codex-message-author">你</span>
        <div className="codex-user-prompt">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown>
        </div>
      </article>
    )
  }

  const live = ["queued", "running", "streaming"].includes(message.status)
  return (
    <article className="codex-turn codex-answer">
      <header className="codex-answer-author">
        <span className="codex-answer-mark">
          <Bot />
        </span>
        <strong>Codex</strong>
      </header>
      {live ? (
        <div className="codex-worklog">
          {message.live_content && (
            <div className="conversation-live-output">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.live_content}</ReactMarkdown>
            </div>
          )}
          <ConversationProgress phase={message.progress_phase} label={message.progress_label} />
        </div>
      ) : (
        <div className="codex-markdown">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown>
        </div>
      )}
      {message.status === "failed" && (
        <p className="message-error">
          <CircleAlert />
          <span>{message.error}</span>
        </p>
      )}
      {message.citations.length > 0 && (
        <div className="citation-list" aria-label="论文引用">
          {message.citations.map((citation) => (
            <button key={citation.id} onClick={() => onCitation(citation)}>
              <strong>第 {citation.page} 页</strong>
              <span>{citation.quote}</span>
            </button>
          ))}
        </div>
      )}
    </article>
  )
}

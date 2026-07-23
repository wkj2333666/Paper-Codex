import type { ChatMessage, MessageCitation } from "./types"

export function latestAnswerCitations(messages: Record<string, ChatMessage>, order: string[]): MessageCitation[] {
  const latestAnswer = [...order].reverse()
    .map(id => messages[id])
    .find(message => message?.role === "assistant")
  if (!latestAnswer) return []
  const unique = new Map<string, MessageCitation>()
  latestAnswer.citations.forEach(citation => unique.set(citation.id, citation))
  return [...unique.values()]
}

export function citationsForPaper(citations: MessageCitation[], paperId: string): MessageCitation[] {
  const unique = new Map<string, MessageCitation>()
  citations.filter(citation => citation.paper_id === paperId).forEach(citation => unique.set(citation.id, citation))
  return [...unique.values()].sort((left, right) =>
    left.page - right.page
    || (left.section ?? "").localeCompare(right.section ?? "")
    || left.id.localeCompare(right.id),
  )
}

export function citationById(citations: MessageCitation[]): Map<string, MessageCitation> {
  return new Map(citations.map(citation => [citation.id, citation]))
}

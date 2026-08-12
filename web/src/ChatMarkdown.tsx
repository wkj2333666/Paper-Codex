import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"

const punctuation = /^\p{P}$/u
const ordinaryText = /^[\p{L}\p{N}]$/u

function codePointBefore(source: string, index: number): string {
  const points = Array.from(source.slice(0, index))
  return points.at(-1) ?? ""
}

function codePointAt(source: string, index: number): string {
  return String.fromCodePoint(source.codePointAt(index) ?? 0)
}

function isEscaped(source: string, index: number): boolean {
  let slashes = 0
  for (let cursor = index - 1; cursor >= 0 && source[cursor] === "\\"; cursor -= 1) slashes += 1
  return slashes % 2 === 1
}

function nextStrongDelimiter(source: string, from: number): number {
  let index = source.indexOf("**", from)
  while (
    index >= 0 &&
    (isEscaped(source, index) || source[index - 1] === "*" || source[index + 2] === "*")
  ) {
    index = source.indexOf("**", index + 2)
  }
  return index
}

function normalizeStrongBoundaries(source: string): string {
  let cursor = 0
  let output = ""
  while (cursor < source.length) {
    const opening = nextStrongDelimiter(source, cursor)
    if (opening < 0) break
    const closing = nextStrongDelimiter(source, opening + 2)
    if (closing < 0) break
    const firstContent = codePointAt(source, opening + 2)
    const beforeClosing = codePointBefore(source, closing)
    const afterClosing = codePointAt(source, closing + 2)
    const content = source.slice(opening + 2, closing)
    const repair =
      content.length > 0 &&
      !content.includes("\n\n") &&
      !/^\s$/u.test(firstContent) &&
      punctuation.test(beforeClosing) &&
      ordinaryText.test(afterClosing)
    output += source.slice(cursor, closing + 2)
    if (repair) output += " "
    cursor = closing + 2
  }
  return output + source.slice(cursor)
}

function backtickRun(source: string, index: number): number {
  let end = index
  while (source[end] === "`") end += 1
  return end - index
}

function closingBackticks(source: string, from: number, length: number): number {
  let cursor = from
  while (cursor < source.length) {
    const index = source.indexOf("`", cursor)
    if (index < 0) return -1
    const run = backtickRun(source, index)
    if (run === length) return index
    cursor = index + run
  }
  return -1
}

function normalizeInlineCode(source: string): string {
  let cursor = 0
  let normalStart = 0
  let output = ""
  while (cursor < source.length) {
    if (source[cursor] !== "`") {
      cursor += 1
      continue
    }
    const length = backtickRun(source, cursor)
    const closing = closingBackticks(source, cursor + length, length)
    if (closing < 0) {
      cursor += length
      continue
    }
    output += normalizeStrongBoundaries(source.slice(normalStart, cursor))
    const protectedEnd = closing + length
    output += source.slice(cursor, protectedEnd)
    cursor = protectedEnd
    normalStart = protectedEnd
  }
  return output + normalizeStrongBoundaries(source.slice(normalStart))
}

interface Fence {
  marker: "`" | "~"
  length: number
}

function fenceAtLineStart(line: string): Fence | null {
  const match = line.match(/^ {0,3}(`{3,}|~{3,})/)
  if (!match) return null
  return { marker: match[1][0] as Fence["marker"], length: match[1].length }
}

function closesFence(line: string, fence: Fence): boolean {
  const match = line.match(/^ {0,3}(`+|~+)\s*$/)
  return Boolean(match && match[1][0] === fence.marker && match[1].length >= fence.length)
}

export function normalizeChatMarkdown(source: string): string {
  let cursor = 0
  let fence: Fence | null = null
  let output = ""
  while (cursor < source.length) {
    const newline = source.indexOf("\n", cursor)
    const end = newline < 0 ? source.length : newline + 1
    const line = source.slice(cursor, end)
    const lineWithoutNewline = line.endsWith("\n") ? line.slice(0, -1) : line
    if (fence) {
      output += line
      if (closesFence(lineWithoutNewline, fence)) fence = null
    } else {
      const opening = fenceAtLineStart(lineWithoutNewline)
      if (opening) {
        fence = opening
        output += line
      } else {
        output += normalizeInlineCode(line)
      }
    }
    cursor = end
  }
  return output
}

export function ChatMarkdown({ children }: { children: string }) {
  return <ReactMarkdown remarkPlugins={[remarkGfm]}>{normalizeChatMarkdown(children)}</ReactMarkdown>
}

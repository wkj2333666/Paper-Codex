import ReactMarkdown from "react-markdown"
import rehypeKatex from "rehype-katex"
import remarkGfm from "remark-gfm"
import remarkMath from "remark-math"

interface MarkdownNode {
  type: string
  value?: string
  children?: MarkdownNode[]
  position?: {
    start: { offset?: number }
    end: { offset?: number }
  }
}

const punctuation = /^[\p{P}\p{S}]$/u
const ordinaryText = /^[\p{L}\p{N}]$/u
const whitespace = /^\p{White_Space}$/u

function codePointBefore(source: string, index: number): string {
  return Array.from(source.slice(0, index)).at(-1) ?? ""
}

function codePointAt(source: string, index: number): string {
  const point = source.codePointAt(index)
  return point === undefined ? "" : String.fromCodePoint(point)
}

function nextStrongDelimiter(source: string, from: number): number {
  let index = source.indexOf("**", from)
  while (index >= 0 && (source[index - 1] === "*" || source[index + 2] === "*")) {
    index = source.indexOf("**", index + 2)
  }
  return index
}

function canOpenStrong(source: string, opening: number): boolean {
  const beforeOpening = codePointBefore(source, opening)
  const afterOpening = codePointAt(source, opening + 2)
  return (
    Boolean(afterOpening) &&
    !whitespace.test(afterOpening) &&
    (!punctuation.test(afterOpening) ||
      !beforeOpening ||
      whitespace.test(beforeOpening) ||
      punctuation.test(beforeOpening))
  )
}

function compatibleStrongText(source: string): MarkdownNode[] {
  const nodes: MarkdownNode[] = []
  let scanCursor = 0
  let emittedCursor = 0
  while (scanCursor < source.length) {
    const opening = nextStrongDelimiter(source, scanCursor)
    if (opening < 0) break
    const closing = nextStrongDelimiter(source, opening + 2)
    if (closing < 0) break
    const content = source.slice(opening + 2, closing)
    const beforeClosing = codePointBefore(source, closing)
    const afterClosing = codePointAt(source, closing + 2)
    const compatible =
      content.length > 0 &&
      !content.includes("\n") &&
      canOpenStrong(source, opening) &&
      punctuation.test(beforeClosing) &&
      ordinaryText.test(afterClosing)
    if (!compatible) {
      scanCursor = closing + 2
      continue
    }
    if (opening > emittedCursor) nodes.push({ type: "text", value: source.slice(emittedCursor, opening) })
    nodes.push({ type: "strong", children: [{ type: "text", value: content }] })
    scanCursor = closing + 2
    emittedCursor = scanCursor
  }
  if (!nodes.length) return [{ type: "text", value: source }]
  if (emittedCursor < source.length) nodes.push({ type: "text", value: source.slice(emittedCursor) })
  return nodes
}

function rawNodeSource(node: MarkdownNode, source: string): string | null {
  const start = node.position?.start.offset
  const end = node.position?.end.offset
  return start === undefined || end === undefined ? null : source.slice(start, end)
}

function repairStrongTextNodes(node: MarkdownNode, source: string): void {
  if (!node.children) return
  node.children = node.children.flatMap((child) => {
    if (
      child.type === "text" &&
      child.value?.includes("**") &&
      rawNodeSource(child, source) === child.value
    ) {
      return compatibleStrongText(child.value)
    }
    repairStrongTextNodes(child, source)
    return [child]
  })
}

function remarkChatMarkdownCompatibility(options: { source: string }) {
  return (tree: MarkdownNode) => repairStrongTextNodes(tree, options.source)
}

export function ChatMarkdown({ children }: { children: string }) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm, remarkMath, [remarkChatMarkdownCompatibility, { source: children }]]}
      rehypePlugins={[rehypeKatex]}
    >
      {children}
    </ReactMarkdown>
  )
}

import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { ChatMarkdown } from "./ChatMarkdown"

const renderMarkdown = (source: string) => renderToStaticMarkup(<ChatMarkdown>{source}</ChatMarkdown>)

describe("ChatMarkdown", () => {
  it.each([
    ["Chinese punctuation", "**结论。**目前", "结论。", "目前"],
    ["ASCII punctuation", "**Conclusion.**Next", "Conclusion.", "Next"],
  ])("repairs strong emphasis followed by text after %s", (_name, source, strong, suffix) => {
    const html = renderMarkdown(source)
    expect(html).toContain(`<strong>${strong}</strong>`)
    expect(html).toContain(suffix)
    expect(html).not.toContain("**")
  })

  it("leaves already valid strong emphasis unchanged", () => {
    expect(renderMarkdown("**结论。** 目前")).toBe("<p><strong>结论。</strong> 目前</p>")
  })

  it("preserves literal stars in inline and fenced code", () => {
    const inline = renderMarkdown("`**结论。**目前`")
    const fenced = renderMarkdown("```text\n**结论。**目前\n```")
    expect(inline).toContain("<code>**结论。**目前</code>")
    expect(fenced).toContain("<pre><code class=\"language-text\">**结论。**目前\n</code></pre>")
  })

  it("preserves multiline code spans and container fences", () => {
    const multiline = renderMarkdown("`code\n**结论。**目前\n`")
    const quotedFence = renderMarkdown("> ```text\n> **结论。**目前\n> ```")
    expect(multiline).toContain("**结论。**目前")
    expect(quotedFence).toContain("<code class=\"language-text\">**结论。**目前\n</code>")
  })

  it("renders inline and display math with KaTeX", () => {
    const html = renderMarkdown("内联 $x^2$。\n\n$$\ny = x + 1\n$$")
    expect(html).toContain('class="katex"')
    expect(html).toContain('class="katex-display"')
    expect(html).not.toContain("$x^2$")
    expect(html).not.toContain("$$")
  })

  it("never rewrites link destinations", () => {
    expect(renderMarkdown("[链接](https://example.test/**a.**b)")).toContain(
      '<a href="https://example.test/**a.**b">链接</a>',
    )
  })

  it("leaves escaped and triple-star delimiters unchanged", () => {
    expect(renderMarkdown("\\**结论。**目前")).toContain("**结论。**目前")
    expect(renderMarkdown("***结论。***目前")).not.toContain("<strong>结论。</strong>")
  })

  it("does not repair a delimiter that cannot open strong emphasis", () => {
    const html = renderMarkdown("a**.foo.**bar")
    expect(html).toContain("a**.foo.**bar")
    expect(html).not.toContain("<strong>")
  })
})

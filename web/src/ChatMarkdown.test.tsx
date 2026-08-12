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
})

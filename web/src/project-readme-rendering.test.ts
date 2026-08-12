// @ts-expect-error Node built-ins are available in Vitest
import { readFileSync } from "node:fs"
import { describe, expect, it } from "vitest"
import * as readmeState from "./project-readme-state"

const editorSource = readFileSync(new URL("./ProjectReadmeEditor.tsx", import.meta.url), "utf8")
const projectViewSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8")
const readmeStyles = readFileSync(new URL("./project-readme.css", import.meta.url), "utf8").replace(/\s+/g, " ")

describe("project README rendering", () => {
  it("normalizes line endings without deleting valid HTML or code", () => {
    const normalize = (readmeState as unknown as { normalizeProjectMarkdown?: (markdown: string) => string }).normalizeProjectMarkdown
    expect(normalize).toBeTypeOf("function")
    if (!normalize) return

    expect(normalize("# Title\r\n\r\n<br />\r\n")).toBe("# Title\n\n<br />\n")
    expect(normalize("first<br />second\r\n")).toBe("first<br />second\n")
    expect(normalize("```html\r\n<br />\r\n```\r\n")).toBe("```html\n<br />\n```\n")
  })

  it("replaces the mounted editor document when server markdown changes", () => {
    expect(editorSource).toContain('import { replaceAll } from "@milkdown/kit/utils"')
    expect(editorSource).toMatch(/editor\.action\(replaceAll\(markdown/)
  })

  it("maps Crepe colors and common Markdown blocks onto the application theme", () => {
    expect(readmeStyles).toMatch(/\.project-readme-crepe \.milkdown\s*\{[^}]*--crepe-color-on-background:\s*var\(--ink\)/)
    expect(readmeStyles).toMatch(/\[data-theme="dark"\] \.project-readme-crepe \.milkdown\s*\{[^}]*--crepe-color-background:\s*var\(--paper-raised\)/)
    expect(readmeStyles).toMatch(/\.project-readme-crepe \.ProseMirror :is\(ul,ol\)\s*\{[^}]*padding-left:/)
    expect(readmeStyles).toMatch(/\.project-readme-crepe \.ProseMirror table\s*\{[^}]*border-collapse:\s*collapse/)
    expect(readmeStyles).toMatch(/\.project-readme-crepe \.ProseMirror pre\s*\{[^}]*overflow-x:\s*auto/)
  })

  it("keeps note text compact and aligns Crepe list labels with their text", () => {
    expect(readmeStyles).toMatch(/\.project-readme-crepe \.ProseMirror\s*\{[^}]*line-height:\s*1\.65/)
    expect(readmeStyles).toMatch(/\.project-readme-crepe \.ProseMirror p\s*\{[^}]*margin:\s*\.55em 0/)
    expect(readmeStyles).toMatch(/\.project-readme-crepe \.ProseMirror li\s*\{[^}]*margin:\s*\.18em 0/)
    expect(readmeStyles).toMatch(/\.project-readme-crepe \.milkdown-list-item-block > \.list-item\s*\{[^}]*gap:\s*\.55rem/)
    expect(readmeStyles).toMatch(/\.project-readme-crepe \.milkdown-list-item-block li\s*\{[^}]*margin:\s*0/)
    expect(readmeStyles).toMatch(/\.project-readme-crepe \.milkdown-list-item-block li \.label-wrapper,\s*\.project-readme-crepe \.milkdown-list-item-block li \.label-wrapper \.label\s*\{[^}]*height:\s*1\.65em/)
    expect(readmeStyles).toMatch(/\.project-readme-crepe \.milkdown-list-item-block li \.label-wrapper \.label\s*\{[^}]*padding:\s*0[^}]*line-height:\s*inherit/)
    expect(readmeStyles).toMatch(/\.project-readme-crepe \.milkdown-list-item-block li > \.children > p\s*\{[^}]*margin:\s*0[^}]*padding:\s*0[^}]*line-height:\s*inherit/)
  })

  it("removes the inherited-context card while retaining the project breadcrumb", () => {
    expect(projectViewSource).not.toContain("继承上下文")
    expect(projectViewSource).not.toContain("inherited-project-context")
    expect(projectViewSource).toContain('aria-label="项目层级"')
  })
})

import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"
import { CandidateCitationList, candidateProjectId } from "./CodexPanel"
import type { CandidateCitation } from "./types"

describe("candidate citations",()=>{
  it("renders external evidence separately as a project candidate",()=>{
    const citation:CandidateCitation={
      id:"candidate-1",
      project_id:"project-a",
      work_id:"work-1",
      title:"Rule Complexity",
      source_url:"https://example.test/rules",
      evidence_level:"abstract",
      quote:"Rules are represented by shortest descriptions.",
      explanation:"直接支持规则描述长度的讨论",
    }
    const open=vi.fn()
    const html=renderToStaticMarkup(<CandidateCitationList citations={[citation]} onCandidate={open}/>)
    expect(html).toContain('aria-label="候选论文：Rule Complexity"')
    expect(html).toContain("已核验摘要")
    expect(html).toContain("直接支持规则描述长度的讨论")
    expect(html).not.toContain("第 0 页")
  })

  it("uses the persisted project even while a paper is open",()=>{
    const citation:CandidateCitation={
      id:"candidate-2",
      project_id:"project-from-conversation",
      work_id:"work-2",
      title:"Prefix Sharing",
      source_url:"https://example.test/prefix",
      evidence_level:"metadata",
      quote:"Shared prefixes reduce repeated work.",
      explanation:"属于会话的项目候选文献",
    }
    const open=vi.fn()
    expect(candidateProjectId(citation)).toBe("project-from-conversation")
    expect(renderToStaticMarkup(<CandidateCitationList citations={[citation]} onCandidate={open}/>)).toContain("Prefix Sharing")
  })
})

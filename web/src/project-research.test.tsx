import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"
import { api } from "./api"
import {
  createCandidateActions,
  ProjectResearchReloadCoordinator,
  ProjectResearchView,
  shouldReloadProjectResearch,
} from "./ProjectResearch"
import type { ProjectCandidate } from "./types"

const candidate = (status:ProjectCandidate["status"]="candidate"):ProjectCandidate=>({
  project_id:"project-a",
  work:{
    id:"work/1",
    canonical_key:"doi:10.1000/rules",
    doi:"10.1000/rules",
    arxiv_id:null,
    openalex_id:null,
    title:"Rule Complexity for Games",
    authors:["Ada"],
    year:2025,
    abstract_text:"Rules can be represented as descriptions.",
    source_url:"https://doi.org/10.1000/rules",
    pdf_url:null,
    evidence_level:"abstract",
    metadata:{},
  },
  status,
  relevance_reason:"直接讨论规则描述复杂度",
  relevance_tags:["游戏规则"],
  evidence_level:"abstract",
  discovered_by_search_run_id:"run-1",
  discovered_by_conversation_id:"conversation-1",
  import_task_id:status==="importing"?"task-1":null,
  paper_id:status==="imported"?"paper-1":null,
  created_at:"2026-07-29T00:00:00Z",
  updated_at:"2026-07-29T00:00:00Z",
})

describe("ProjectResearch",()=>{
  it("recognizes a selected project's research revision as a reload signal",()=>{
    expect(shouldReloadProjectResearch({type:"project-research-revised",payload:{project_id:"project-a",revision:2}},"project-a")).toBe(true)
    expect(shouldReloadProjectResearch({type:"project-research-revised",payload:{project_id:"project-b",revision:2}},"project-a")).toBe(false)
    expect(shouldReloadProjectResearch({type:"done",payload:{project_id:"project-a"}},"project-a")).toBe(false)
  })

  it("keeps candidate actions scoped to the selected project",async()=>{
    const update=vi.spyOn(api,"updateCandidate").mockResolvedValue(candidate("dismissed"))
    const refresh=vi.fn(async()=>{})
    await createCandidateActions("project-a",refresh,vi.fn()).dismiss("work/1")
    expect(update).toHaveBeenCalledWith("project-a","work/1",{status:"dismissed"})
  })

  it("does not let an older research reload overwrite a newer response",async()=>{
    let resolveFirst!:(value:string)=>void
    let resolveSecond!:(value:string)=>void
    const first=new Promise<string>(resolve=>{resolveFirst=resolve})
    const second=new Promise<string>(resolve=>{resolveSecond=resolve})
    const applied:string[]=[]
    const coordinator=new ProjectResearchReloadCoordinator()

    const firstRun=coordinator.run(()=>first,value=>applied.push(value))
    const secondRun=coordinator.run(()=>second,value=>applied.push(value))
    resolveSecond("newer")
    expect(await secondRun).toBe(true)
    resolveFirst("older")
    expect(await firstRun).toBe(false)
    expect(applied).toEqual(["newer"])
  })

  it("only acknowledges a research reload after it succeeds",async()=>{
    const coordinator=new ProjectResearchReloadCoordinator()
    expect(await coordinator.run(async()=>{throw new Error("temporary")},()=>{})).toBe(false)
    const applied:string[]=[]
    expect(await coordinator.run(async()=>"retried",value=>applied.push(value))).toBe(true)
    expect(applied).toEqual(["retried"])
  })

  it("distinguishes candidate, importing, and imported actions",()=>{
    const html=renderToStaticMarkup(<ProjectResearchView
      tab="candidates"
      papers={[]}
      candidates={[candidate(),candidate("importing"),candidate("imported")]}
      searches={[]}
      includeDismissed={false}
      busy={false}
      error=""
      onTab={()=>{}}
      onOpenCandidate={()=>{}}
      onOpenPaper={()=>{}}
      onRemovePaper={()=>{}}
      onToggleDismissed={()=>{}}
      actions={createCandidateActions("project-a",async()=>{},()=>{})}
      onOpenSearch={()=>{}}
    />)
    expect(html).toContain("Rule Complexity for Games")
    expect(html).toContain("加入项目并分析")
    expect(html).toContain("正在导入")
    expect(html).toContain("打开项目论文")
    expect(html).toContain("显示暂不考虑")
    expect(html).toContain("全部添加（1）")
  })

  it("renders the project README workspace in a dedicated notes tab",()=>{
    const html=renderToStaticMarkup(<ProjectResearchView
      tab="notes"
      papers={[]}
      candidates={[]}
      searches={[]}
      includeDismissed={false}
      busy={false}
      error=""
      onTab={()=>{}}
      onOpenCandidate={()=>{}}
      onOpenPaper={()=>{}}
      onRemovePaper={()=>{}}
      onToggleDismissed={()=>{}}
      actions={createCandidateActions("project-a",async()=>{},()=>{})}
      onOpenSearch={()=>{}}
      notes={<div>README editor</div>}
    />)
    expect(html).toContain("项目笔记")
    expect(html).toContain("README editor")
  })

  it("keeps the project README mounted while another project tab is visible",()=>{
    const html=renderToStaticMarkup(<ProjectResearchView
      tab="overview"
      papers={[]}
      candidates={[]}
      searches={[]}
      includeDismissed={false}
      busy={false}
      error=""
      onTab={()=>{}}
      onOpenCandidate={()=>{}}
      onOpenPaper={()=>{}}
      onRemovePaper={()=>{}}
      onToggleDismissed={()=>{}}
      actions={createCandidateActions("project-a",async()=>{},()=>{})}
      onOpenSearch={()=>{}}
      overview={<div>Overview</div>}
      notes={<div>README editor</div>}
    />)
    expect(html).toContain("Overview")
    expect(html).toContain("README editor")
    expect(html).toContain("project-notes-panel")
    expect(html).toContain("hidden")
  })

  it("shows per-paper failures after adding every candidate",()=>{
    const html=renderToStaticMarkup(<ProjectResearchView
      tab="candidates"
      papers={[]}
      candidates={[candidate()]}
      searches={[]}
      includeDismissed={false}
      busy={false}
      error=""
      onTab={()=>{}}
      onOpenCandidate={()=>{}}
      onOpenPaper={()=>{}}
      onRemovePaper={()=>{}}
      onToggleDismissed={()=>{}}
      actions={createCandidateActions("project-a",async()=>{},()=>{})}
      onOpenSearch={()=>{}}
      bulkResult={{total:1,succeeded:0,failed:1,items:[{work_id:"work/1",outcome:null,error:"PDF 地址不可用"}]}}
    />)
    expect(html).toContain("批量添加失败明细")
    expect(html).toContain("Rule Complexity for Games")
    expect(html).toContain("PDF 地址不可用")
  })
})

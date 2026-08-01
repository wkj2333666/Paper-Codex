import { describe, expect, it } from "vitest"
import { buildProjectOverview } from "./project-overview"
import type { GraphPayload, ProjectCandidate } from "./types"

const graph:GraphPayload={
  nodes:[
    {id:"paper:a",kind:"paper",label:"核心论文",description:"",paper_id:"paper:a"},
    {id:"method:m",kind:"method",label:"共享前缀",description:"",paper_id:null},
    {id:"finding:f",kind:"finding",label:"吞吐提升",description:"",paper_id:null},
  ],
  edges:[
    {id:"e1",source:"paper:a",target:"method:m",relation_type:"uses",hypothesis:false,confidence:.9,evidence:[]},
    {id:"e2",source:"method:m",target:"finding:f",relation_type:"may-improve",hypothesis:true,confidence:.6,evidence:[]},
  ],
}

const candidate:ProjectCandidate={
  project_id:"project:one",status:"importing",relevance_reason:"可验证共享前缀对数值稳定性的影响",relevance_tags:["数值稳定性"],evidence_level:"fulltext",
  discovered_by_search_run_id:null,discovered_by_conversation_id:"conversation:one",import_task_id:"task:one",paper_id:null,created_at:"",updated_at:"",
  work:{id:"work:one",canonical_key:"doi:one",doi:"one",arxiv_id:null,openalex_id:null,title:"数值稳定性研究",authors:[],year:2025,abstract_text:"摘要",source_url:"https://example.test",pdf_url:"https://example.test/p.pdf",evidence_level:"fulltext",metadata:{}},
}

describe("project overview projection",()=>{
  it("prioritizes active goals and imports and derives graph-backed directions",()=>{
    const overview=buildProjectOverview({
      project:{id:"project:one",slug:"one",name:"共享前缀",purpose:"研究共享前缀推理",parent_id:null,created_at:"",updated_at:""},
      papers:[{id:"paper:a",title:"核心论文",authors_json:"[]",year:2024,doi:null,arxiv_id:null,canonical_sha256:"rev",source_url:null,note_path:"note",deleted_at:null,created_at:"",updated_at:""}],
      candidates:[candidate],searches:[],graph,
      goals:[{conversation_id:"conversation:one",conversation_title:"数值一致性",objective:"完成共享前缀论文综述",status:"active",tokens_used:1200,time_used_seconds:30,updated_at:""}],
    })
    expect(overview.metrics).toEqual({papers:1,candidates:1,activeGoals:1,hypotheses:1})
    expect(overview.importantWork.slice(0,2).map(item=>item.kind)).toEqual(["goal","import"])
    expect(overview.keyPapers[0]).toMatchObject({id:"paper:a",connections:1})
    expect(overview.directions[0].title).toContain("共享前缀")
    expect(overview.directions[0].title).toContain("吞吐提升")
  })
})

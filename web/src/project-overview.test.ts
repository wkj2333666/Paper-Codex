import { describe, expect, it } from "vitest"
import { buildProjectOverview } from "./project-overview"
import type { GraphPayload, ProjectCandidate } from "./types"

const graph:GraphPayload={
  nodes:[
    {id:"paper:a",kind:"paper",label:"补充论文",description:"",paper_id:"paper:a"},
    {id:"paper:b",kind:"paper",label:"核心论文",description:"",paper_id:"paper:b"},
    {id:"method:m",kind:"method",label:"共享前缀",description:"",paper_id:null},
    {id:"finding:f",kind:"finding",label:"吞吐提升",description:"",paper_id:null},
  ],
  edges:[
    {id:"e1",source:"paper:a",target:"method:m",relation_type:"uses",hypothesis:false,confidence:.9,evidence:[]},
    {id:"e2",source:"paper:b",target:"method:m",relation_type:"uses",hypothesis:false,confidence:.9,evidence:[]},
    {id:"e3",source:"paper:b",target:"finding:f",relation_type:"reports",hypothesis:false,confidence:.8,evidence:[]},
    {id:"e4",source:"method:m",target:"finding:f",relation_type:"may-improve",hypothesis:true,confidence:.6,evidence:[]},
  ],
}

function candidate(id:string,status:ProjectCandidate["status"]):ProjectCandidate{
  return {
    project_id:"project:one",status,relevance_reason:"可验证共享前缀对数值稳定性的影响",relevance_tags:["数值稳定性"],evidence_level:"fulltext",
    discovered_by_search_run_id:null,discovered_by_conversation_id:"conversation:one",import_task_id:status==="importing"?`task:${id}`:null,paper_id:null,created_at:"",updated_at:"",
    work:{id:`work:${id}`,canonical_key:`doi:${id}`,doi:id,arxiv_id:null,openalex_id:null,title:`候选 ${id}`,authors:[],year:2025,abstract_text:"摘要",source_url:"https://example.test",pdf_url:"https://example.test/p.pdf",evidence_level:"fulltext",metadata:{}},
  }
}

describe("project overview projection",()=>{
  it("treats formal papers as important works and keeps activity in progress",()=>{
    const overview=buildProjectOverview({
      project:{id:"project:one",slug:"one",name:"共享前缀",purpose:"研究共享前缀推理",parent_id:null,created_at:"",updated_at:""},
      papers:[
        {id:"paper:a",title:"补充论文",authors_json:"[]",year:2023,doi:null,arxiv_id:null,canonical_sha256:"rev-a",source_url:null,note_path:"note-a",deleted_at:null,created_at:"",updated_at:""},
        {id:"paper:b",title:"核心论文",authors_json:"[]",year:2024,doi:null,arxiv_id:null,canonical_sha256:"rev-b",source_url:null,note_path:"note-b",deleted_at:null,created_at:"",updated_at:""},
      ],
      candidates:[candidate("importing","importing"),candidate("candidate","candidate")],
      searches:[{id:"search:one",project_id:"project:one",conversation_id:"conversation:one",message_id:"message:one",trigger:"automatic",question:"寻找共享前缀论文",query_plan:{},state:"running",provider_status:{},error:null,created_at:"",updated_at:""}],
      graph,
      goals:[{conversation_id:"conversation:one",conversation_title:"数值一致性",objective:"完成共享前缀论文综述",status:"active",tokens_used:1200,time_used_seconds:30,updated_at:""}],
    })

    expect(overview.metrics).toEqual({papers:2,candidates:2,activeGoals:1,hypotheses:1})
    expect(overview.importantWorks).toEqual([
      {id:"paper:b",title:"核心论文",year:2024,connections:2},
      {id:"paper:a",title:"补充论文",year:2023,connections:1},
    ])
    expect(overview.progress.map(item=>item.kind)).toEqual(["goal","import","search"])
    expect(overview.progress.map(item=>item.title)).not.toContain("候选 candidate")
    expect(overview.directions[0].title).toContain("共享前缀")
    expect(overview.directions[0].title).toContain("吞吐提升")
  })
})

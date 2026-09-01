import { describe, expect, it, vi } from "vitest"
import { routeIntakeSubmission } from "./intake-flow"
import type { IntakeSearchResponse } from "./types"

const oneCandidate:IntakeSearchResponse={
  query:"jepa",
  state:"completed",
  providers:{fixture:{state:"completed",hits:1,error:null}},
  results:[{
    work:{
      id:"work-1",canonical_key:"arxiv:2301.08243",doi:null,arxiv_id:"2301.08243",openalex_id:null,
      title:"I-JEPA",authors:["Mahmoud Assran"],year:2023,abstract_text:null,
      source_url:"https://arxiv.org/abs/2301.08243",pdf_url:"https://arxiv.org/pdf/2301.08243.pdf",
      evidence_level:"abstract",metadata:{},
    },
    providers:["fixture"],best_rank:1,provider_scores:{fixture:1},raw_results:[],
    match:{score:0.9,title_exact:false},fulltext:{state:"available",source_count:1},
  }],
}

describe("routeIntakeSubmission",()=>{
  it("turns free text into candidates even when exactly one result is returned",async()=>{
    const intake=vi.fn(async()=>{throw {status:422,body:{code:"intake_search_required"}}})
    const searchIntake=vi.fn(async()=>oneCandidate)

    const result=await routeIntakeSubmission("jepa","project-1",{intake,searchIntake})

    expect(result).toEqual({state:"candidates",response:oneCandidate})
    expect(intake).toHaveBeenCalledWith("jepa","project-1")
    expect(searchIntake).toHaveBeenCalledWith("jepa",12)
  })

  it("keeps direct identifiers on the enqueue path",async()=>{
    const intake=vi.fn(async()=>({kind:"enqueued" as const,task_id:"task-1"}))
    const searchIntake=vi.fn(async()=>oneCandidate)

    const result=await routeIntakeSubmission("arxiv:2301.08243",undefined,{intake,searchIntake})

    expect(result).toEqual({state:"enqueued",task_id:"task-1"})
    expect(searchIntake).not.toHaveBeenCalled()
  })
})

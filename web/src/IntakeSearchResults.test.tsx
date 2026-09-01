import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { IntakeSearchResults } from "./IntakeSearchResults"
import type { IntakeSearchResponse } from "./types"

const response:IntakeSearchResponse={
  query:"jepa",
  state:"partial",
  providers:{
    arxiv:{state:"completed",hits:1,error:null},
    openalex:{state:"failed",hits:0,error:"HTTP 429: rate limit exceeded"},
  },
  results:[{
    work:{
      id:"work-1",canonical_key:"arxiv:2301.08243",doi:null,arxiv_id:"2301.08243",openalex_id:null,
      title:"I-JEPA: Self-Supervised Learning from Images",authors:["Mahmoud Assran","Quentin Duval"],year:2023,
      abstract_text:"Joint embedding predictive architecture",source_url:"https://arxiv.org/abs/2301.08243",
      pdf_url:"https://arxiv.org/pdf/2301.08243.pdf",evidence_level:"abstract",metadata:{},
    },
    providers:["arxiv"],best_rank:1,provider_scores:{arxiv:1},raw_results:[],
    match:{score:0.88,title_exact:false},fulltext:{state:"available",source_count:1},
  }],
}

describe("IntakeSearchResults",()=>{
  it("renders candidates, provider degradation, and an explicit import action",()=>{
    const html=renderToStaticMarkup(<IntakeSearchResults response={response} importingWorkId={null} importError="" onImport={()=>{}} onDismiss={()=>{}}/>)
    expect(html).toContain("I-JEPA: Self-Supervised Learning from Images")
    expect(html).toContain("Mahmoud Assran")
    expect(html).toContain("OpenAlex 暂时失败")
    expect(html).toContain("导入并分析")
    expect(html).not.toContain("正在处理")
  })

  it("keeps an empty successful search distinct from a provider failure",()=>{
    const html=renderToStaticMarkup(<IntakeSearchResults response={{...response,state:"completed",providers:{},results:[]}} importingWorkId={null} importError="" onImport={()=>{}} onDismiss={()=>{}}/>)
    expect(html).toContain("没有找到匹配论文")
    expect(html).toContain("换一个完整标题、作者名或 DOI 再试")
  })
})

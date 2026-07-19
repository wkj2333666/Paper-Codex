import { describe, expect, test } from "vitest"
import { briefFromAnalysis, describePaperImpact } from "./reading"

describe("paper reading helpers",()=>{
  test("keeps the default brief to one takeaway and at most three bullets per section",()=>{
    const brief=briefFromAnalysis({
      takeaway:"一句话结论",
      research_question:"研究问题",
      method:"第一步\n第二步\n第三步\n第四步",
      results:["结果一","结果二","结果三","结果四"],
      limitations:["限制一"],
    })
    expect(brief.takeaway).toBe("一句话结论")
    expect(brief.method).toEqual(["第一步","第二步","第三步"])
    expect(brief.results).toEqual(["结果一","结果二","结果三"])
  })

  test("fills missing legacy fields without exposing metadata",()=>{
    const brief=briefFromAnalysis({contribution:"核心贡献"})
    expect(brief.takeaway).toBe("核心贡献")
    expect(brief.researchQuestion).toEqual(["尚待 Codex 补充"])
    expect(JSON.stringify(brief)).not.toContain("paper_id")
  })

  test("removes evidence attribution labels from the reading brief",()=>{
    const brief=briefFromAnalysis({
      takeaway:"[作者结论；证据 E2、E8] Transformer 提高了训练并行性。",
      research_question:"[作者问题设定；证据 E2] 能否完全移除循环结构？",
      results:["[作者实验；证据 E8] 英德翻译达到 28.4 BLEU。"],
    })
    expect(brief.takeaway).toBe("Transformer 提高了训练并行性。")
    expect(brief.researchQuestion).toEqual(["能否完全移除循环结构？"])
    expect(brief.results).toEqual(["英德翻译达到 28.4 BLEU。"])
  })

  test("describes deletion impact in Chinese",()=>{
    expect(describePaperImpact({project_references:2,graph_edges:5,revisions:1}))
      .toBe("涉及 2 个项目引用、5 条图谱关系和 1 个论文版本")
  })
})

import type { PaperAnalysis, PaperImpact } from "./types"

export interface PaperBrief {
  takeaway:string
  researchQuestion:string[]
  method:string[]
  results:string[]
  limitations:string[]
}

function points(value:unknown):string[]{
  const values=Array.isArray(value)?value:typeof value==="string"?value.split(/\n+/):[]
  return values
    .map(item=>cleanDisplayText(String(item).replace(/^[-*•]\s*/,"")))
    .filter(Boolean)
    .slice(0,3)
}

export function cleanDisplayText(value:string):string{
  return value
    .replace(/\[(?=[^\]]*(?:作者|分析者|证据\s*E))[^\]]+\]\s*/g,"")
    .replace(/[（(]证据\s*E[\d、—–,，\s]+[）)]/g,"")
    .trim()
}

const fallback=(value:string[])=>value.length?value:["尚待 Codex 补充"]

export function briefFromAnalysis(analysis:PaperAnalysis|null|undefined):PaperBrief{
  const value=analysis??{}
  const takeaway=cleanDisplayText(String(value.takeaway??value.contribution??""))||"尚待 Codex 生成一句话结论"
  return {
    takeaway,
    researchQuestion:fallback(points(value.research_question)),
    method:fallback(points(value.method)),
    results:fallback(points(value.results)),
    limitations:fallback(points(value.limitations)),
  }
}

export function describePaperImpact(impact:PaperImpact):string{
  return `涉及 ${impact.project_references} 个项目引用、${impact.graph_edges} 条图谱关系和 ${impact.revisions} 个论文版本`
}

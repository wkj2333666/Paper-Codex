import type { GraphPayload, LiteratureSearchRun, Paper, Project, ProjectCandidate, ProjectGoalSummary } from "./types"

export interface ProjectOverviewInput {
  project:Project
  papers:Paper[]
  candidates:ProjectCandidate[]
  searches:LiteratureSearchRun[]
  graph:GraphPayload
  goals:ProjectGoalSummary[]
}

export interface ProjectOverviewProgressItem {
  id:string
  kind:"goal"|"import"|"search"
  title:string
  detail:string
}

export function buildProjectOverview(input:ProjectOverviewInput){
  const activeCandidates=input.candidates.filter(item=>item.status!=="dismissed"&&item.status!=="imported")
  const hypotheses=input.graph.edges.filter(edge=>edge.hypothesis)
  const progress:ProjectOverviewProgressItem[]=[]
  for(const goal of input.goals.filter(item=>item.status==="active"))progress.push({id:`goal:${goal.conversation_id}`,kind:"goal",title:goal.objective,detail:`Goal · ${goal.conversation_title}`})
  for(const candidate of activeCandidates.filter(item=>item.status==="importing"))progress.push({id:`import:${candidate.work.id}`,kind:"import",title:candidate.work.title,detail:"正在导入、评阅并写入知识图谱"})
  for(const search of input.searches.filter(item=>item.state==="running"))progress.push({id:`search:${search.id}`,kind:"search",title:search.question,detail:"检索正在进行"})

  const degrees=new Map<string,number>()
  for(const edge of input.graph.edges){degrees.set(edge.source,(degrees.get(edge.source)??0)+1);degrees.set(edge.target,(degrees.get(edge.target)??0)+1)}
  const paperNodeIds=new Map<string,string[]>()
  for(const node of input.graph.nodes.filter(node=>node.kind==="paper"&&node.paper_id)){
    const ids=paperNodeIds.get(node.paper_id!)??[]
    ids.push(node.id)
    paperNodeIds.set(node.paper_id!,ids)
  }
  const importantWorks=input.papers.map(paper=>({
    id:paper.id,
    title:paper.title,
    year:paper.year,
    connections:Math.max(0,...(paperNodeIds.get(paper.id)??[paper.id]).map(nodeId=>degrees.get(nodeId)??0)),
  })).sort((left,right)=>right.connections-left.connections||left.title.localeCompare(right.title)).slice(0,6)
  const nodeLabels=new Map(input.graph.nodes.map(node=>[node.id,node.label]))
  const directions=hypotheses.sort((left,right)=>right.confidence-left.confidence).slice(0,5).map(edge=>({
    id:edge.id,
    title:`验证“${nodeLabels.get(edge.source)??edge.source}”与“${nodeLabels.get(edge.target)??edge.target}”的联系`,
    detail:`待验证关系：${edge.relation_type} · 置信度 ${Math.round(edge.confidence*100)}%`,
  }))
  for(const candidate of activeCandidates.filter(item=>item.status==="candidate").slice(0,Math.max(0,5-directions.length)))directions.push({id:`candidate:${candidate.work.id}`,title:`评估候选：${candidate.work.title}`,detail:candidate.relevance_reason})
  if(!directions.length&&input.project.purpose.trim())directions.push({id:"project-purpose",title:`推进：${input.project.purpose}`,detail:"可在项目 Codex 对话中设置 Goal，让 Codex 自动检索并整理论文。"})

  return {
    metrics:{papers:input.papers.length,candidates:activeCandidates.length,activeGoals:input.goals.filter(item=>item.status==="active").length,hypotheses:hypotheses.length},
    importantWorks,
    progress,
    directions,
  }
}

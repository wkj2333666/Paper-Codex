import type { Paper, Project } from "./types"

export interface InheritedProjectPapers {
  project:Project
  papers:Paper[]
}

export function projectBreadcrumb(projects:Project[],projectId:string):Project[]{
  const byId=new Map(projects.map(project=>[project.id,project]))
  const path:Project[]=[]
  const seen=new Set<string>()
  let current=byId.get(projectId)
  while(current&&!seen.has(current.id)){
    seen.add(current.id)
    path.unshift(current)
    current=current.parent_id?byId.get(current.parent_id):undefined
  }
  return path
}

export function inheritedProjectPapers(
  projects:Project[],
  memberships:Record<string,string[]>,
  papers:Paper[],
  projectId:string,
):InheritedProjectPapers[]{
  const byPaperId=new Map(papers.map(paper=>[paper.id,paper]))
  return projectBreadcrumb(projects,projectId).slice(0,-1).map(project=>({
    project,
    papers:(memberships[project.id]??[]).map(id=>byPaperId.get(id)).filter((paper):paper is Paper=>Boolean(paper)),
  })).filter(group=>group.papers.length>0)
}

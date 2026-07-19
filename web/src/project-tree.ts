import type { Project } from "./types"

export interface ProjectTreeNode extends Project {
  children: ProjectTreeNode[]
  directPaperCount: number
  paperCount: number
}

const compareProjects=(left:Project,right:Project)=>left.name.localeCompare(right.name,"zh-CN")

export function buildProjectTree(
  projects:Project[],
  memberships:Record<string,string[]> = {},
):ProjectTreeNode[] {
  const known=new Set(projects.map(project=>project.id))
  const children=new Map<string|null,Project[]>()
  for(const project of projects){
    const parent=project.parent_id&&known.has(project.parent_id)?project.parent_id:null
    const list=children.get(parent)??[]
    list.push(project)
    children.set(parent,list)
  }
  for(const list of children.values())list.sort(compareProjects)
  const visit=(project:Project,ancestors:Set<string>):ProjectTreeNode=>{
    if(ancestors.has(project.id))return {...project,children:[],directPaperCount:memberships[project.id]?.length??0,paperCount:memberships[project.id]?.length??0}
    const next=new Set(ancestors).add(project.id)
    const nested=(children.get(project.id)??[]).map(child=>visit(child,next))
    const directPaperCount=memberships[project.id]?.length??0
    return {...project,children:nested,directPaperCount,paperCount:directPaperCount+nested.reduce((sum,child)=>sum+child.paperCount,0)}
  }
  return (children.get(null)??[]).map(project=>visit(project,new Set()))
}

export function descendantIds(projects:Project[],id:string):Set<string>{
  const result=new Set<string>()
  const pending=[id]
  while(pending.length){
    const parent=pending.pop()!
    for(const project of projects){
      if(project.parent_id===parent&&!result.has(project.id)){
        result.add(project.id)
        pending.push(project.id)
      }
    }
  }
  return result
}

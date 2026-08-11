import { describe, expect, it } from "vitest"
import { inheritedProjectPapers, projectBreadcrumb } from "./project-context"
import type { Paper, Project } from "./types"

const project=(id:string,parent_id:string|null):Project=>({
  id,slug:id,name:id,purpose:"",parent_id,created_at:"",updated_at:"",
})

const paper=(id:string):Paper=>({
  id,title:id,authors_json:"[]",year:null,doi:null,arxiv_id:null,
  canonical_sha256:null,source_url:null,note_path:null,deleted_at:null,
  created_at:"",updated_at:"",
})

describe("project context",()=>{
  const projects=[
    project("root",null),
    project("child","root"),
    project("grandchild","child"),
    project("sibling","root"),
    project("descendant","grandchild"),
  ]
  const papers=[paper("root-paper"),paper("child-paper"),paper("own-paper"),paper("sibling-paper"),paper("descendant-paper")]
  const memberships={
    root:["root-paper"],
    child:["child-paper"],
    grandchild:["own-paper"],
    sibling:["sibling-paper"],
    descendant:["descendant-paper"],
  }

  it("returns a root-to-current breadcrumb",()=>{
    expect(projectBreadcrumb(projects,"grandchild").map(item=>item.id)).toEqual(["root","child","grandchild"])
  })

  it("returns only ancestor direct papers as inherited context",()=>{
    expect(inheritedProjectPapers(projects,memberships,papers,"grandchild").map(group=>({
      project:group.project.id,
      papers:group.papers.map(item=>item.id),
    }))).toEqual([
      {project:"root",papers:["root-paper"]},
      {project:"child",papers:["child-paper"]},
    ])
  })

  it("stops safely when stored ancestry contains a cycle",()=>{
    const cyclic=[project("a","b"),project("b","a")]
    expect(projectBreadcrumb(cyclic,"a").map(item=>item.id)).toEqual(["b","a"])
  })
})

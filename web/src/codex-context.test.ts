import { describe, expect, it } from "vitest"
import { projectIdForSelection, withProjectContext } from "./codex-context"
import type { Dashboard } from "./types"

const dashboard:Dashboard={
  papers:[],
  projects:[
    {id:"project-a",slug:"a",name:"项目 A",purpose:"",parent_id:null,created_at:"",updated_at:""},
    {id:"project-b",slug:"b",name:"项目 B",purpose:"",parent_id:null,created_at:"",updated_at:""},
  ],
  tasks:[],
  inbox:[],
  trash_count:0,
  project_memberships:{},
}

describe("Codex project context",()=>{
  it("keeps the active project when a paper is opened",()=>{
    const selection=withProjectContext({kind:"paper",id:"paper:one"},"project-b",dashboard)
    expect(selection).toEqual({kind:"paper",id:"paper:one",projectId:"project-b"})
    expect(projectIdForSelection(selection)).toBe("project-b")
  })

  it("uses the selected project as the persistent context",()=>{
    expect(withProjectContext({kind:"project",id:"project-a"},"project-b",dashboard))
      .toEqual({kind:"project",id:"project-a",projectId:"project-a"})
  })

  it("defaults to an existing project when opening the workspace",()=>{
    expect(withProjectContext({kind:"workbench"},undefined,dashboard))
      .toEqual({kind:"workbench",projectId:"project-a"})
  })
})

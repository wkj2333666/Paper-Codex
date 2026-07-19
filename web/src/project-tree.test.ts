import { describe, expect, test } from "vitest"
import type { Project } from "./types"
import { buildProjectTree, descendantIds } from "./project-tree"

const project = (id:string,name:string,parent_id:string|null):Project => ({
  id,slug:id,name,purpose:"",parent_id,created_at:"",updated_at:"",
})

describe("project tree",()=>{
  test("builds stable nested nodes and keeps orphaned projects visible",()=>{
    const projects=[
      project("child-b","子项目 B","root"),
      project("orphan","孤立项目","missing"),
      project("root","根项目",null),
      project("child-a","子项目 A","root"),
    ]
    const tree=buildProjectTree(projects,{root:["p1"],"child-a":["p2","p3"]})
    expect(tree.map(node=>node.id)).toEqual(["root","orphan"])
    expect(tree[0].children.map(node=>node.id)).toEqual(["child-a","child-b"])
    expect(tree[0].paperCount).toBe(3)
    expect(tree[0].directPaperCount).toBe(1)
  })

  test("returns every descendant for move-cycle prevention",()=>{
    const projects=[project("a","A",null),project("b","B","a"),project("c","C","b")]
    expect([...descendantIds(projects,"a")].sort()).toEqual(["b","c"])
  })
})

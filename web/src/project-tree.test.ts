import { describe, expect, test } from "vitest"
import type { Project } from "./types"
import { buildProjectTree, descendantIds, planProjectMove, projectDropPosition } from "./project-tree"

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

  test("saved sibling order overrides names without detaching subtrees",()=>{
    const tree=buildProjectTree([
      {...project("a","A",null),sort_order:1},
      {...project("z","Z",null),sort_order:0},
      project("child","Child","a"),
    ])
    expect(tree.map(node=>node.id)).toEqual(["z","a"])
    expect(tree[1].children[0].id).toBe("child")
  })

  test("row edges insert before/after and the middle reparents",()=>{
    expect(projectDropPosition(102,100,40)).toBe("before")
    expect(projectDropPosition(120,100,40)).toBe("inside")
    expect(projectDropPosition(138,100,40)).toBe("after")
    const projects=[project("a","A",null),project("b","B",null),project("c","C",null),project("child","Child","b")]
    expect(planProjectMove(projects,"c","a","before")).toEqual({parent_id:null,ordered_ids:["c","a","b"]})
    expect(planProjectMove(projects,"a","b","after")).toEqual({parent_id:null,ordered_ids:["b","a","c"]})
    expect(planProjectMove(projects,"a","b","inside")).toEqual({parent_id:"b",ordered_ids:["child","a"]})
    expect(planProjectMove(projects,"child",null,"inside")).toEqual({parent_id:null,ordered_ids:["a","b","c","child"]})
    expect(planProjectMove(projects,"c","child","before")).toEqual({parent_id:"b",ordered_ids:["c","child"]})
  })

  test("rejects cycles, self drops and missing projects",()=>{
    const projects=[project("a","A",null),project("b","B","a"),project("c","C","b")]
    for(const position of ["before","inside","after"] as const){
      expect(planProjectMove(projects,"a","c",position)).toBeNull()
      expect(planProjectMove(projects,"a","a",position)).toBeNull()
    }
    expect(planProjectMove(projects,"missing","a","inside")).toBeNull()
    expect(planProjectMove(projects,"a","missing","before")).toBeNull()
  })

  test("moves use all siblings, including those hidden by a UI search",()=>{
    const projects=[project("a","A",null),project("b","B",null),project("c","C",null)]
    expect(planProjectMove(projects,"c","a","after")?.ordered_ids).toEqual(["a","c","b"])
  })
})

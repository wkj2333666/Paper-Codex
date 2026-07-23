import { describe, expect, test } from "vitest"
import type { GraphPayload } from "./types"
import { buildGraph, graphPalette, initialPosition, kindColor, kindLabel, neighborhood } from "./graph-model"

const payload:GraphPayload={
  nodes:[
    {id:"paper:one",kind:"paper",label:"论文一",description:"",paper_id:"paper:one"},
    {id:"method:attention",kind:"method",label:"注意力",description:"方法",paper_id:null},
    {id:"finding:parallel",kind:"finding",label:"可并行",description:"发现",paper_id:null},
  ],
  edges:[
    {id:"formal",source:"paper:one",target:"method:attention",relation_type:"uses-method",hypothesis:false,confidence:.98,evidence:[]},
    {id:"guess",source:"method:attention",target:"finding:parallel",relation_type:"supports",hypothesis:true,confidence:.55,evidence:[]},
  ],
}

describe("semantic graph model",()=>{
  test("assigns deterministic positions, Chinese labels, and degree-based node sizes",()=>{
    expect(initialPosition("paper:one")).toEqual(initialPosition("paper:one"))
    expect(kindLabel("finding")).toBe("研究发现")
    const graph=buildGraph(payload)
    expect(graph.getNodeAttribute("method:attention","size"))
      .toBeGreaterThan(graph.getNodeAttribute("paper:one","size"))
    expect(graph.getNodeAttribute("method:attention","color")).toBe("#a66a2c")
  })

  test("keeps formal and hypothesis edges visually and semantically distinct",()=>{
    const graph=buildGraph(payload)
    expect(graph.getEdgeAttribute("formal","hypothesis")).toBe(false)
    expect(graph.getEdgeAttribute("formal","dashed")).toBe(false)
    expect(graph.getEdgeAttribute("guess","hypothesis")).toBe(true)
    expect(graph.getEdgeAttribute("guess","dashed")).toBe(true)
    expect(graph.getEdgeAttribute("guess","color")).toContain("0.35")
  })

  test("returns the hovered node and its immediate neighborhood",()=>{
    const graph=buildGraph(payload)
    expect([...neighborhood(graph,"method:attention")].sort())
      .toEqual(["finding:parallel","method:attention","paper:one"])
  })

  test("keeps multiple typed relations between the same pair of nodes",()=>{
    const graph=buildGraph({...payload,edges:[...payload.edges,{
      id:"second-formal",source:"paper:one",target:"method:attention",relation_type:"introduces",hypothesis:false,confidence:.9,evidence:[],
    }]})
    expect(graph.edges("paper:one","method:attention").sort()).toEqual(["formal","second-formal"])
  })

  test("uses a high-contrast palette for dark graph surfaces",()=>{
    const palette=graphPalette("dark")
    expect(palette.canvasBackground).toBe("#202a25")
    expect(kindColor("paper",palette)).toBe("#78c7a0")
    expect(kindColor("method",palette)).toBe("#f2b56f")
    expect(palette.dimNodeColor).not.toMatch(/^#([0-9a-f]{2})\1\1$/i)
    const graph=buildGraph(payload,palette)
    expect(graph.getNodeAttribute("method:attention","color")).toBe("#f2b56f")
    expect(graph.getEdgeAttribute("formal","color")).toBe(palette.edgeColor)
    expect(graph.getEdgeAttribute("guess","color")).toBe(palette.hypothesisEdgeColor)
  })
})

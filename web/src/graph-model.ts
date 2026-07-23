import Graph from "graphology"
import type { GraphPayload, KnowledgeKind } from "./types"
import type { ResolvedTheme } from "./theme"

export interface GraphPalette {
  nodeColors:Record<KnowledgeKind,string>
  edgeColor:string
  hypothesisEdgeColor:string
  dimNodeColor:string
  canvasBackground:string
  labelColor:string
}

const LIGHT_PALETTE:GraphPalette={
  nodeColors:{
    paper:"#2f5d4a",
    concept:"#6d5b9c",
    method:"#a66a2c",
    dataset:"#2f6f83",
    finding:"#9a4f52",
  },
  edgeColor:"rgba(70,78,69,0.62)",
  hypothesisEdgeColor:"rgba(166,106,44,0.35)",
  dimNodeColor:"#d8d8d0",
  canvasBackground:"#f4f6f2",
  labelColor:"#20382d",
}

const DARK_PALETTE:GraphPalette={
  nodeColors:{
    paper:"#78c7a0",
    concept:"#b7a7f0",
    method:"#f2b56f",
    dataset:"#71c6df",
    finding:"#f08f9b",
  },
  edgeColor:"rgba(190,218,201,0.72)",
  hypothesisEdgeColor:"rgba(246,201,113,0.86)",
  dimNodeColor:"#53645a",
  canvasBackground:"#202a25",
  labelColor:"#eef7f0",
}

const COLORS:Record<KnowledgeKind,string>=LIGHT_PALETTE.nodeColors

const LABELS:Record<KnowledgeKind,string>={
  paper:"论文",
  concept:"概念",
  method:"方法",
  dataset:"数据集",
  finding:"研究发现",
}

export const kindLabel=(kind:KnowledgeKind)=>LABELS[kind]
export const graphPalette=(theme:ResolvedTheme):GraphPalette=>theme==="dark"?DARK_PALETTE:LIGHT_PALETTE
export const kindColor=(kind:KnowledgeKind,palette:GraphPalette=LIGHT_PALETTE)=>palette.nodeColors[kind]??COLORS[kind]

export function initialPosition(id:string):{x:number;y:number}{
  let first=2166136261,second=5381
  for(let index=0;index<id.length;index++){
    const code=id.charCodeAt(index)
    first=Math.imul(first^code,16777619)
    second=((second<<5)+second)^code
  }
  return {x:(first>>>0)/0xffffffff*100,y:(second>>>0)/0xffffffff*100}
}

export function buildGraph(payload:GraphPayload,palette:GraphPalette=LIGHT_PALETTE):Graph{
  const graph=new Graph({multi:true,type:"directed"})
  const degree=new Map<string,number>()
  for(const edge of payload.edges){
    degree.set(edge.source,(degree.get(edge.source)??0)+1)
    degree.set(edge.target,(degree.get(edge.target)??0)+1)
  }
  for(const node of payload.nodes){
    const position=initialPosition(node.id)
    graph.addNode(node.id,{
      ...position,
      label:node.label,
      description:node.description,
      kind:node.kind,
      paperId:node.paper_id,
      color:kindColor(node.kind,palette),
      size:7+Math.sqrt(degree.get(node.id)??0)*3,
    })
  }
  for(const edge of payload.edges){
    if(!graph.hasNode(edge.source)||!graph.hasNode(edge.target)||graph.hasEdge(edge.id))continue
    graph.addEdgeWithKey(edge.id,edge.source,edge.target,{
      relationType:edge.relation_type,
      hypothesis:edge.hypothesis,
      dashed:edge.hypothesis,
      confidence:edge.confidence,
      evidence:edge.evidence,
      color:edge.hypothesis?palette.hypothesisEdgeColor:palette.edgeColor,
      size:edge.hypothesis?1:1.6,
    })
  }
  return graph
}

export function neighborhood(graph:Graph,node:string):Set<string>{
  if(!graph.hasNode(node))return new Set()
  return new Set([node,...graph.neighbors(node)])
}

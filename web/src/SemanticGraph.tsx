import { useEffect, useMemo, useState } from "react"
import { SigmaContainer, useLoadGraph, useRegisterEvents, useSigma } from "@react-sigma/core"
import "@react-sigma/core/lib/style.css"
import forceAtlas2 from "graphology-layout-forceatlas2"
import { Focus, Maximize2, Search, ZoomIn, ZoomOut } from "lucide-react"
import { buildGraph, graphPalette, kindColor, kindLabel, type GraphPalette } from "./graph-model"
import type { ResolvedTheme } from "./theme"
import type { GraphNode, GraphPayload, KnowledgeKind } from "./types"

interface Props {
  payload:GraphPayload
  theme?:ResolvedTheme
  compact?:boolean
  focusNode?:string
  onOpenFull?:()=>void
  onPaperOpen?:(paperId:string)=>void
  onSelectionChange?:(node:GraphNode|null)=>void
}

function Loader({payload,compact,palette}:{payload:GraphPayload;compact:boolean;palette:GraphPalette}){
  const loadGraph=useLoadGraph()
  useEffect(()=>{
    const graph=buildGraph(payload,palette)
    if(graph.order>1){
      forceAtlas2.assign(graph,{
        iterations:compact?45:Math.min(140,70+graph.order),
        settings:{...forceAtlas2.inferSettings(graph),gravity:1,scalingRatio:2,strongGravityMode:true,barnesHutOptimize:graph.order>40},
      })
    }
    loadGraph(graph)
  },[payload,compact,loadGraph,palette])
  return null
}

function Events({setHovered,select}:{setHovered:(id:string|null)=>void;select:(id:string)=>void}){
  const registerEvents=useRegisterEvents()
  useEffect(()=>registerEvents({
    enterNode:({node})=>setHovered(node),
    leaveNode:()=>setHovered(null),
    clickNode:({node})=>select(node),
  }),[registerEvents,setHovered,select])
  return null
}

function Controls(){
  const sigma=useSigma()
  return <div className="graph-controls">
    <button title="放大" onClick={()=>void sigma.getCamera().animatedZoom()}><ZoomIn/></button>
    <button title="缩小" onClick={()=>void sigma.getCamera().animatedUnzoom()}><ZoomOut/></button>
    <button title="适应画布" onClick={()=>void sigma.getCamera().animatedReset()}><Focus/></button>
  </div>
}

export function SemanticGraph({payload,theme="light",compact=false,focusNode,onOpenFull,onPaperOpen,onSelectionChange}:Props){
  const palette=useMemo(()=>graphPalette(theme),[theme])
  const [hovered,setHovered]=useState<string|null>(null)
  const [selected,setSelected]=useState<string|null>(focusNode??null)
  const [query,setQuery]=useState("")
  const nodes=useMemo(()=>new Map(payload.nodes.map(node=>[node.id,node])),[payload.nodes])
  const edgeEndpoints=useMemo(()=>new Map(payload.edges.map(edge=>[edge.id,[edge.source,edge.target] as const])),[payload.edges])
  const neighborhood=useMemo(()=>{
    if(!hovered)return null
    const values=new Set([hovered])
    for(const edge of payload.edges){
      if(edge.source===hovered)values.add(edge.target)
      if(edge.target===hovered)values.add(edge.source)
    }
    return values
  },[hovered,payload.edges])
  const select=(id:string)=>{
    const node=nodes.get(id)??null
    setSelected(id)
    onSelectionChange?.(node)
    if(node?.paper_id&&onPaperOpen)onPaperOpen(node.paper_id)
  }
  const settings=useMemo(()=>({
    renderEdgeLabels:false,
    labelDensity:compact?.4:.7,
    labelGridCellSize:compact?120:90,
    defaultNodeColor:palette.nodeColors.paper,
    defaultEdgeColor:palette.edgeColor,
    labelColor:{color:palette.labelColor},
    nodeReducer:(node:string,data:Record<string,unknown>)=>{
      if(neighborhood&&!neighborhood.has(node))return {...data,color:palette.dimNodeColor,label:""}
      if(node===selected)return {...data,size:Number(data.size??8)*1.35,highlighted:true}
      return data
    },
    edgeReducer:(edge:string,data:Record<string,unknown>)=>{
      const endpoints=edgeEndpoints.get(edge)
      if(neighborhood&&endpoints&&(!neighborhood.has(endpoints[0])||!neighborhood.has(endpoints[1])))return {...data,hidden:true}
      return data
    },
  }),[compact,edgeEndpoints,neighborhood,palette,selected])
  const matches=query.trim()?payload.nodes.filter(node=>node.label.toLowerCase().includes(query.trim().toLowerCase())).slice(0,6):[]
  const selectedNode=selected?nodes.get(selected):undefined
  return <div className={compact?"semantic-graph compact":"semantic-graph"} style={{background:palette.canvasBackground}}>
    {!compact&&<div className="graph-search"><Search/><input value={query} onChange={event=>setQuery(event.target.value)} placeholder="搜索论文、方法、概念、数据集或发现"/>{matches.length>0&&<div className="graph-search-results">{matches.map(node=><button key={node.id} onClick={()=>select(node.id)}><span style={{background:kindColor(node.kind,palette)}}/>{node.label}<small>{kindLabel(node.kind)}</small></button>)}</div>}</div>}
    {onOpenFull&&<button className="graph-open" onClick={onOpenFull}><Maximize2/>{compact?"打开全局图谱":"全屏查看"}</button>}
    <SigmaContainer className="sigma-canvas" settings={settings}>
      <Loader payload={payload} compact={compact} palette={palette}/><Events setHovered={setHovered} select={select}/><Controls/>
    </SigmaContainer>
    {payload.nodes.length===0&&<div className="graph-empty">尚无图谱数据。Codex 完成论文分析后会自动建立连接。</div>}
    {!compact&&<div className="graph-legend">{(["paper","concept","method","dataset","finding"] as KnowledgeKind[]).map(kind=><span key={kind}><i style={{background:kindColor(kind,palette)}}/>{kindLabel(kind)}</span>)}<span><i className="edge-solid"/>有证据</span><span><i className="edge-dashed"/>假设</span></div>}
    {selectedNode&&!compact&&<div className="graph-node-card"><span>{kindLabel(selectedNode.kind)}</span><strong>{selectedNode.label}</strong><p>{selectedNode.description||"暂无补充说明"}</p></div>}
  </div>
}

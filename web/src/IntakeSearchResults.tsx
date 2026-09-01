import { CircleAlert, FileSearch, LoaderCircle, X } from "lucide-react"
import type { IntakeSearchResponse, IntakeSearchResult } from "./types"

const PROVIDER_NAMES:Record<string,string>={arxiv:"arXiv",crossref:"Crossref",openalex:"OpenAlex",openreview:"OpenReview"}
const providerName=(provider:string)=>PROVIDER_NAMES[provider]??provider

function authors(result:IntakeSearchResult):string{
  const visible=result.work.authors.slice(0,3)
  const overflow=result.work.authors.length-visible.length
  return `${visible.join("、")}${overflow>0?` 等 ${overflow+visible.length} 位作者`:""}`
}

function identifier(result:IntakeSearchResult):string{
  if(result.work.doi)return `DOI ${result.work.doi}`
  if(result.work.arxiv_id)return `arXiv ${result.work.arxiv_id}`
  return result.work.canonical_key
}

function fulltextLabel(result:IntakeSearchResult):string{
  if(result.fulltext.state==="available")return `可直接获取 PDF${result.fulltext.source_count>1?` · ${result.fulltext.source_count} 个来源`:""}`
  if(result.fulltext.state==="possible")return "可能可获取全文"
  return "暂无可用 PDF"
}

export function IntakeSearchResults({response,importingWorkId,importError,onImport,onDismiss}:{response:IntakeSearchResponse;importingWorkId:string|null;importError:string;onImport:(workId:string)=>void;onDismiss:()=>void}){
  const failedProviders=Object.entries(response.providers).filter(([,status])=>status.state==="failed")
  return <section className="intake-search-results" aria-label="论文候选">
    <header><div><strong>找到 {response.results.length} 篇候选</strong><span>“{response.query}”</span></div><button type="button" aria-label="关闭候选列表" onClick={onDismiss}><X/></button></header>
    {failedProviders.length>0&&<div className="intake-provider-warning"><CircleAlert/>{failedProviders.map(([provider])=>`${providerName(provider)} 暂时失败`).join("；")}，已保留其他来源的结果。</div>}
    {importError&&<div className="intake-search-error"><CircleAlert/>{importError}</div>}
    {response.results.length===0?<div className="intake-search-empty"><FileSearch/><strong>没有找到匹配论文</strong><span>换一个完整标题、作者名或 DOI 再试</span></div>:<div className="intake-candidate-list">{response.results.map(result=>{
      const importing=importingWorkId===result.work.id
      return <article key={result.work.id} className="intake-candidate">
        <div className="intake-candidate-main"><div className="intake-candidate-title"><h3>{result.work.title}</h3>{result.match.score<.45&&<span className="weak-match">匹配较弱</span>}</div>
          <p>{authors(result)||"作者未知"}{result.work.year?` · ${result.work.year}`:""}</p>
          <div className="intake-candidate-meta"><span>{identifier(result)}</span><span>{fulltextLabel(result)}</span>{result.providers.map(provider=><em key={provider}>{providerName(provider)}</em>)}</div>
        </div>
        <button type="button" disabled={!!importingWorkId||result.fulltext.state==="unavailable"} onClick={()=>onImport(result.work.id)}>{importing?<><LoaderCircle className="spin"/>正在导入</>:"导入并分析"}</button>
      </article>
    })}</div>}
  </section>
}

import type { Dashboard, GraphPayload, Paper, PaperDetail, PaperImpact, Project, ProjectImpact, SearchResult, StreamEvent, Task } from "./types"

export class ApiError extends Error { constructor(public status:number,message:string){super(message)} }
const TOKEN_KEY = "paper-codex-token"
const TOKEN_HEADER = "x-paper-codex-token"
export const session = { get:()=>localStorage.getItem(TOKEN_KEY), set:(token:string)=>localStorage.setItem(TOKEN_KEY,token), clear:()=>localStorage.removeItem(TOKEN_KEY) }

async function request<T>(path:string,init:RequestInit={}):Promise<T> {
  const headers = new Headers(init.headers); const token=session.get(); if(token) headers.set(TOKEN_HEADER,token)
  if(init.body && !(init.body instanceof FormData)) headers.set("content-type","application/json")
  const response=await fetch(path,{...init,headers});
  if(!response.ok){ if(response.status===401) session.clear(); const body=await response.json().catch(()=>({})); throw new ApiError(response.status,body.error ?? `HTTP ${response.status}`) }
  if(response.status===204) return undefined as T
  return response.json() as Promise<T>
}

export const api = {
  async login(password:string){ const result=await request<{token:string}>("/api/session",{method:"POST",body:JSON.stringify({password})}); session.set(result.token); return result },
  dashboard:()=>request<Dashboard>("/api/dashboard"),
  paper:(id:string)=>request<PaperDetail>(`/api/paper?id=${encodeURIComponent(id)}`),
  tasks:()=>request<Task[]>("/api/tasks"),
  intake:(source:string,project_id?:string)=>request<{task_id:string}>("/api/intake",{method:"POST",body:JSON.stringify({source,project_id:project_id||null})}),
  upload(file:File,project_id?:string){const body=new FormData();body.append("file",file);if(project_id)body.append("project_id",project_id);return request<{task_id:string}>("/api/intake/upload",{method:"POST",body})},
  createProject:(name:string,purpose:string,parent_id?:string|null)=>request<Project>("/api/projects",{method:"POST",body:JSON.stringify({name,purpose,parent_id:parent_id??null})}),
  updateProject:(id:string,value:{name:string;purpose:string;parent_id:string|null})=>request<Project>(`/api/projects/${encodeURIComponent(id)}`,{method:"PATCH",body:JSON.stringify(value)}),
  deleteProject:(id:string,subtree=false)=>request<void>(`/api/projects/${encodeURIComponent(id)}${subtree?"?mode=subtree":""}`,{method:"DELETE"}),
  projectImpact:(id:string)=>request<ProjectImpact>(`/api/projects/${encodeURIComponent(id)}/impact`),
  addPaper:(projectId:string,paperId:string)=>request<void>(`/api/projects/${encodeURIComponent(projectId)}/papers/${encodeURIComponent(paperId)}`,{method:"POST"}),
  removePaper:(projectId:string,paperId:string)=>request<void>(`/api/projects/${encodeURIComponent(projectId)}/papers/${encodeURIComponent(paperId)}`,{method:"DELETE"}),
  trash:()=>request<Paper[]>("/api/trash"),
  paperImpact:(id:string)=>request<PaperImpact>(`/api/paper/impact?id=${encodeURIComponent(id)}`),
  trashPaper:(id:string)=>request<void>(`/api/paper?id=${encodeURIComponent(id)}`,{method:"DELETE"}),
  restorePaper:(id:string)=>request<void>(`/api/paper/restore?id=${encodeURIComponent(id)}`,{method:"POST"}),
  permanentlyDeletePaper:(id:string)=>request<void>(`/api/paper/permanent?id=${encodeURIComponent(id)}`,{method:"DELETE"}),
  graph:(options:{project_id?:string;paper_id?:string;kinds?:string[];include_hypotheses?:boolean}={})=>{
    const query=new URLSearchParams()
    if(options.project_id)query.set("project_id",options.project_id)
    if(options.paper_id)query.set("paper_id",options.paper_id)
    if(options.kinds?.length)query.set("kinds",options.kinds.join(","))
    if(options.include_hypotheses!==undefined)query.set("include_hypotheses",String(options.include_hypotheses))
    return request<GraphPayload>(`/api/graph${query.size?`?${query}`:""}`)
  },
  search:(query:string)=>request<SearchResult[]>(`/api/search?q=${encodeURIComponent(query)}`),
  question:(scope_type:string,scope_id:string|null,question:string)=>request<{task_id:string}>("/api/questions",{method:"POST",body:JSON.stringify({scope_type,scope_id,question})}),
  async pdf(id:string){const token=session.get();const response=await fetch(`/api/paper/pdf?id=${encodeURIComponent(id)}`,{headers:token?{[TOKEN_HEADER]:token}:{}});if(!response.ok)throw new ApiError(response.status,"PDF unavailable");return response.blob()},
}

export async function streamEvents(after:number,onEvent:(event:StreamEvent)=>void,signal:AbortSignal):Promise<void>{
  const token=session.get();const response=await fetch(`/api/events?after=${after}`,{headers:token?{[TOKEN_HEADER]:token}:{},signal});
  if(!response.ok||!response.body)throw new ApiError(response.status,"event stream unavailable")
  const reader=response.body.getReader(),decoder=new TextDecoder();let buffer=""
  while(true){const {done,value}=await reader.read();if(done)break;buffer+=decoder.decode(value,{stream:true});let end:number
    while((end=buffer.indexOf("\n\n"))>=0){const block=buffer.slice(0,end);buffer=buffer.slice(end+2);let id=0,type="message",data="";
      for(const line of block.split("\n")){if(line.startsWith("id:"))id=Number(line.slice(3).trim());else if(line.startsWith("event:"))type=line.slice(6).trim();else if(line.startsWith("data:"))data+=line.slice(5).trim()}
      if(data){const value=JSON.parse(data);onEvent({id,type,task_id:value.task_id,payload:value.payload??{},created_at:value.created_at??""})}
    }
  }
}

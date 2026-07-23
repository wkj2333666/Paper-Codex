import type { Annotation, AnnotationAnchor, Conversation, ConversationDetail, ConversationScope, ConversationStreamEvent, Dashboard, GraphPayload, Paper, PaperAnnotation, PaperDetail, PaperImpact, Project, ProjectImpact, SearchResult, StreamEvent, Task } from "./types"

export class ApiError extends Error { constructor(public status:number,message:string){super(message)} }
const TOKEN_KEY = "paper-codex-token"
const TOKEN_HEADER = "x-paper-codex-token"
export const session = { get:()=>localStorage.getItem(TOKEN_KEY), set:(token:string)=>localStorage.setItem(TOKEN_KEY,token), clear:()=>localStorage.removeItem(TOKEN_KEY) }
export const authHeaders=():Record<string,string>=>{const token=session.get();return token?{[TOKEN_HEADER]:token}:{}}
export const pdfUrl=(id:string)=>`/api/paper/pdf?id=${encodeURIComponent(id)}`

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
  cancelTask:(id:string)=>request<void>(`/api/tasks/${encodeURIComponent(id)}/cancel`,{method:"POST"}),
  dismissTask:(id:string)=>request<void>(`/api/tasks/${encodeURIComponent(id)}`,{method:"DELETE"}),
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
  conversations:(archived=false)=>request<Conversation[]>(`/api/conversations?archived=${archived}`),
  createConversation:(title:string,scopes:ConversationScope[])=>request<Conversation>("/api/conversations",{method:"POST",body:JSON.stringify({title,scopes})}),
  conversation:(id:string)=>request<ConversationDetail>(`/api/conversations/${encodeURIComponent(id)}`),
  updateConversation:(id:string,value:{title?:string;archived?:boolean})=>request<Conversation>(`/api/conversations/${encodeURIComponent(id)}`,{method:"PATCH",body:JSON.stringify(value)}),
  replaceConversationScopes:(id:string,scopes:ConversationScope[])=>request<ConversationScope[]>(`/api/conversations/${encodeURIComponent(id)}/scopes`,{method:"PUT",body:JSON.stringify({scopes})}),
  sendConversationMessage:(id:string,content:string)=>request<{message_id:string;status:string}>(`/api/conversations/${encodeURIComponent(id)}/messages`,{method:"POST",body:JSON.stringify({content})}),
  cancelConversation:(id:string)=>request<void>(`/api/conversations/${encodeURIComponent(id)}/cancel`,{method:"POST"}),
  pinCitation:(id:string)=>request<Annotation>(`/api/citations/${encodeURIComponent(id)}/pin`,{method:"POST"}),
  paperAnnotations:(paperId:string)=>request<PaperAnnotation[]>(`/api/paper/annotations?id=${encodeURIComponent(paperId)}`),
  updateAnnotation:(id:string,state:"visible"|"hidden")=>request<Annotation>(`/api/annotations/${encodeURIComponent(id)}`,{method:"PATCH",body:JSON.stringify({state})}),
  replaceAnnotationAnchors:(id:string,anchors:Omit<AnnotationAnchor,"annotation_id">[])=>request<void>(`/api/annotations/${encodeURIComponent(id)}/anchors`,{method:"PUT",body:JSON.stringify({anchors})}),
  async pdf(id:string){const response=await fetch(pdfUrl(id),{headers:authHeaders()});if(!response.ok)throw new ApiError(response.status,"PDF unavailable");return response.blob()},
  pdfUrl,
  authHeaders,
}

export async function streamConversationEvents(id:string,after:number,onEvent:(event:ConversationStreamEvent)=>void,signal:AbortSignal):Promise<void>{
  const token=session.get();const response=await fetch(`/api/conversations/${encodeURIComponent(id)}/events?after=${after}`,{headers:token?{[TOKEN_HEADER]:token}:{},signal})
  if(!response.ok||!response.body)throw new ApiError(response.status,"conversation stream unavailable")
  const reader=response.body.getReader(),decoder=new TextDecoder();let buffer=""
  while(true){const {done,value}=await reader.read();if(done)break;buffer+=decoder.decode(value,{stream:true});let end:number
    while((end=buffer.indexOf("\n\n"))>=0){const block=buffer.slice(0,end);buffer=buffer.slice(end+2);let eventId=0,type="message",data=""
      for(const line of block.split("\n")){if(line.startsWith("id:"))eventId=Number(line.slice(3).trim());else if(line.startsWith("event:"))type=line.slice(6).trim();else if(line.startsWith("data:"))data+=line.slice(5).trim()}
      if(data){const parsed=JSON.parse(data);onEvent({id:eventId,type,conversation_id:parsed.conversation_id,message_id:parsed.message_id??null,payload:parsed.payload??{},created_at:parsed.created_at??""})}
    }
  }
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

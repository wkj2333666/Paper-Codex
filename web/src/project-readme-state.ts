import type { ProjectReadme } from "./types"

export type ProjectReadmeStatus="loading"|"saved"|"dirty"|"saving"|"conflict"|"error"

export interface ProjectReadmeState {
  status:ProjectReadmeStatus
  markdown:string
  savedMarkdown:string
  revision:string
  requestId:number
  savingMarkdown:string|null
  conflictRevision:string|null
  error:string|null
}

export type ProjectReadmeAction=
  |{type:"loading"}
  |{type:"loaded";value:ProjectReadme}
  |{type:"edit";markdown:string}
  |{type:"saving";requestId:number}
  |{type:"saved";requestId:number;value:ProjectReadme}
  |{type:"conflict";requestId:number;currentRevision:string}
  |{type:"failed";requestId:number;error:string}

export const initialProjectReadmeState:ProjectReadmeState={
  status:"loading",markdown:"",savedMarkdown:"",revision:"",requestId:0,
  savingMarkdown:null,conflictRevision:null,error:null,
}

export function projectReadmeReducer(state:ProjectReadmeState,action:ProjectReadmeAction):ProjectReadmeState{
  if(action.type==="loading")return {...initialProjectReadmeState}
  if(action.type==="loaded")return {
    ...initialProjectReadmeState,status:"saved",markdown:action.value.markdown,
    savedMarkdown:action.value.markdown,revision:action.value.revision,
  }
  if(action.type==="edit")return {
    ...state,markdown:action.markdown,
    status:state.status==="conflict"?"conflict":state.status==="saving"?"saving":action.markdown===state.savedMarkdown?"saved":"dirty",
    error:null,
  }
  if(action.type==="saving")return {
    ...state,status:"saving",requestId:action.requestId,savingMarkdown:state.markdown,error:null,
  }
  if(action.requestId!==state.requestId)return state
  if(action.type==="saved")return {
    ...state,status:state.markdown===action.value.markdown?"saved":"dirty",
    savedMarkdown:action.value.markdown,revision:action.value.revision,
    savingMarkdown:null,conflictRevision:null,error:null,
  }
  if(action.type==="conflict")return {
    ...state,status:"conflict",savingMarkdown:null,
    conflictRevision:action.currentRevision,error:"项目笔记已在其他位置更新",
  }
  return {...state,status:"error",savingMarkdown:null,error:action.error}
}

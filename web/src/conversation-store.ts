import type { CandidateCitation, ChatMessage, Conversation, ConversationDetail, ConversationScope, ConversationStreamEvent, CodexRunSettings, MessageCitation, ResearchProgressPhase } from "./types"
import type { CodexSelection } from "./conversation-scope"

export interface PendingConversationSwitch {
  requestId: number
  conversationId: string
  targetSelection: CodexSelection|null
  status: "loading"|"resolved"
}

export interface ConversationState {
  conversations: Conversation[]
  activeConversationId: string|null
  activeSettings: CodexRunSettings|null
  scopes: ConversationScope[]
  messages: Record<string,ChatMessage>
  messageOrder: string[]
  drawerOpen: boolean
  drawerView: "history"|"activity"
  lastEventId: number
  pendingSwitch: PendingConversationSwitch|null
}

export const conversationInitialState:ConversationState={conversations:[],activeConversationId:null,activeSettings:null,scopes:[],messages:{},messageOrder:[],drawerOpen:false,drawerView:"history",lastEventId:0,pendingSwitch:null}

export type ConversationAction=
  |{type:"conversations";items:Conversation[]}
  |{type:"active";id:string|null}
  |{type:"detail";detail:ConversationDetail}
  |{type:"hydrate-detail";expectedConversationId:string;detail:ConversationDetail}
  |{type:"switch-start";requestId:number;conversationId:string}
  |{type:"switch-resolved";requestId:number;detail:ConversationDetail;targetSelection:CodexSelection|null}
  |{type:"switch-failed";requestId:number}
  |{type:"switch-complete";requestId:number}
  |{type:"drawer";open:boolean;view?:"history"|"activity"}
  |{type:"event";event:ConversationStreamEvent}

function pendingMessage(id:string,conversationId:string):ChatMessage{return {id,conversation_id:conversationId,role:"assistant",content:"",live_content:"",turn_id:null,status:"streaming",error:null,research_mode:"auto",tool_preferences:[],citations:[],candidate_citations:[],created_at:"",updated_at:""}}

const researchProgressPhases:ResearchProgressPhase[]=["research-planning","research-searching","research-deduplicating","research-inspecting-abstract","research-fetching-fulltext","research-saving-candidates","research-partial"]
function progressPhase(value:unknown):ChatMessage["progress_phase"]{return value==="reading"||value==="reasoning"||value==="tool"||value==="answering"||researchProgressPhases.includes(value as ResearchProgressPhase)?value as ChatMessage["progress_phase"]:undefined}

export function reduceConversationEvent(state:ConversationState,event:ConversationStreamEvent):ConversationState{
  if(event.id<=state.lastEventId)return state
  const messageId=event.message_id
  if(!messageId)return {...state,lastEventId:event.id}
  const current=state.messages[messageId]??pendingMessage(messageId,event.conversation_id)
  let next=current
  if(event.type==="answer-queued")next={...current,status:"queued"}
  else if(event.type==="answer-started")next={...current,status:"running",progress_phase:"reasoning",progress_label:"Codex 已开始处理问题…"}
  else if(event.type==="answer-progress")next={...current,status:"streaming",progress_phase:progressPhase(event.payload.phase)??"reasoning",progress_label:String(event.payload.label??"")||undefined}
  else if(event.type==="answer-delta")next={...current,status:"streaming",live_content:`${current.live_content??""}${String(event.payload.text??"")}`,progress_phase:"answering",progress_label:"Codex 正在生成回答…"}
  else if(event.type==="answer-completed")next={...current,status:"completed",content:String(event.payload.answer_markdown??""),live_content:undefined,citations:(event.payload.citations as MessageCitation[]|undefined)??[],candidate_citations:(event.payload.candidate_citations as CandidateCitation[]|undefined)??[],progress_phase:undefined,progress_label:undefined}
  else if(event.type==="answer-failed")next={...current,status:"failed",live_content:undefined,error:String(event.payload.message??"回答失败"),progress_phase:undefined,progress_label:undefined}
  else if(event.type==="answer-cancelled")next={...current,status:"cancelled",live_content:undefined,progress_phase:undefined,progress_label:undefined}
  else if(event.type==="message-created"){
    const skill=event.payload.skill as {name?:unknown}|null|undefined
    next={...current,role:(event.payload.role as ChatMessage["role"])??"user",content:String(event.payload.content??""),skill_name:typeof skill?.name==="string"?skill.name:null,tool_preferences:(event.payload.tool_preferences as ChatMessage["tool_preferences"]|undefined)??[],status:"completed"}
  }
  const exists=state.messageOrder.includes(messageId)
  return {...state,lastEventId:event.id,messages:{...state.messages,[messageId]:next},messageOrder:exists?state.messageOrder:[...state.messageOrder,messageId]}
}

function installDetail(state:ConversationState,detail:ConversationDetail):ConversationState{
  const messages=Object.fromEntries(detail.messages.map(message=>[message.id,message]))
  const {model,reasoning_effort,service_tier}=detail.conversation
  const activeSettings=model&&reasoning_effort?{model,reasoning_effort,service_tier}:null
  return {...state,activeConversationId:detail.conversation.id,activeSettings,scopes:detail.scopes,messages,messageOrder:detail.messages.map(message=>message.id)}
}

export function conversationReducer(state:ConversationState,action:ConversationAction):ConversationState{
  if(action.type==="conversations")return {...state,conversations:action.items}
  if(action.type==="active"){
    if(action.id===state.activeConversationId)return state
    return {...state,activeConversationId:action.id,activeSettings:null,scopes:[],messages:{},messageOrder:[],lastEventId:0,drawerOpen:false,pendingSwitch:null}
  }
  if(action.type==="switch-start")return {...state,pendingSwitch:{requestId:action.requestId,conversationId:action.conversationId,targetSelection:null,status:"loading"}}
  if(action.type==="switch-resolved"){
    if(state.pendingSwitch?.requestId!==action.requestId)return state
    const installed=installDetail(state,action.detail)
    return {...installed,drawerOpen:false,pendingSwitch:{requestId:action.requestId,conversationId:action.detail.conversation.id,targetSelection:action.targetSelection,status:"resolved"}}
  }
  if(action.type==="switch-failed"){
    if(state.pendingSwitch?.requestId!==action.requestId)return state
    return {...state,pendingSwitch:null}
  }
  if(action.type==="switch-complete"){
    if(state.pendingSwitch?.requestId!==action.requestId)return state
    return {...state,pendingSwitch:null}
  }
  if(action.type==="hydrate-detail"){
    if(state.activeConversationId!==action.expectedConversationId||action.detail.conversation.id!==action.expectedConversationId)return state
    return installDetail(state,action.detail)
  }
  if(action.type==="drawer")return {...state,drawerOpen:action.open,drawerView:action.view??state.drawerView}
  if(action.type==="event")return reduceConversationEvent(state,action.event)
  return {...installDetail(state,action.detail),pendingSwitch:null}
}

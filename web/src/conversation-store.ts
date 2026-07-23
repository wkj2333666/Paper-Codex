import type { ChatMessage, Conversation, ConversationDetail, ConversationScope, ConversationStreamEvent, CodexRunSettings, MessageCitation } from "./types"

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
}

export const conversationInitialState:ConversationState={conversations:[],activeConversationId:null,activeSettings:null,scopes:[],messages:{},messageOrder:[],drawerOpen:false,drawerView:"history",lastEventId:0}

export type ConversationAction=
  |{type:"conversations";items:Conversation[]}
  |{type:"active";id:string|null}
  |{type:"detail";detail:ConversationDetail}
  |{type:"drawer";open:boolean;view?:"history"|"activity"}
  |{type:"event";event:ConversationStreamEvent}

function pendingMessage(id:string,conversationId:string):ChatMessage{return {id,conversation_id:conversationId,role:"assistant",content:"",live_content:"",turn_id:null,status:"streaming",error:null,citations:[],created_at:"",updated_at:""}}

function progressPhase(value:unknown):ChatMessage["progress_phase"]{return value==="reading"||value==="reasoning"||value==="tool"||value==="answering"?value:undefined}

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
  else if(event.type==="answer-completed")next={...current,status:"completed",content:String(event.payload.answer_markdown??""),live_content:undefined,citations:(event.payload.citations as MessageCitation[]|undefined)??[],progress_phase:undefined,progress_label:undefined}
  else if(event.type==="answer-failed")next={...current,status:"failed",live_content:undefined,error:String(event.payload.message??"回答失败"),progress_phase:undefined,progress_label:undefined}
  else if(event.type==="answer-cancelled")next={...current,status:"cancelled",live_content:undefined,progress_phase:undefined,progress_label:undefined}
  else if(event.type==="message-created")next={...current,role:(event.payload.role as ChatMessage["role"])??"user",content:String(event.payload.content??""),status:"completed"}
  const exists=state.messageOrder.includes(messageId)
  return {...state,lastEventId:event.id,messages:{...state.messages,[messageId]:next},messageOrder:exists?state.messageOrder:[...state.messageOrder,messageId]}
}

export function conversationReducer(state:ConversationState,action:ConversationAction):ConversationState{
  if(action.type==="conversations")return {...state,conversations:action.items}
  if(action.type==="active")return {...state,activeConversationId:action.id,activeSettings:null,scopes:[],messages:{},messageOrder:[],lastEventId:0,drawerOpen:false}
  if(action.type==="drawer")return {...state,drawerOpen:action.open,drawerView:action.view??state.drawerView}
  if(action.type==="event")return reduceConversationEvent(state,action.event)
  const messages=Object.fromEntries(action.detail.messages.map(message=>[message.id,message]))
  const {model,reasoning_effort,service_tier}=action.detail.conversation
  const activeSettings=model&&reasoning_effort?{model,reasoning_effort,service_tier}:null
  return {...state,activeConversationId:action.detail.conversation.id,activeSettings,scopes:action.detail.scopes,messages,messageOrder:action.detail.messages.map(message=>message.id)}
}

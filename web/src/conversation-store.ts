import type { ChatMessage, Conversation, ConversationDetail, ConversationScope, ConversationStreamEvent, MessageCitation } from "./types"

export interface ConversationState {
  conversations: Conversation[]
  activeConversationId: string|null
  scopes: ConversationScope[]
  messages: Record<string,ChatMessage>
  messageOrder: string[]
  drawerOpen: boolean
  drawerView: "history"|"activity"
  lastEventId: number
}

export const conversationInitialState:ConversationState={conversations:[],activeConversationId:null,scopes:[],messages:{},messageOrder:[],drawerOpen:false,drawerView:"history",lastEventId:0}

export type ConversationAction=
  |{type:"conversations";items:Conversation[]}
  |{type:"active";id:string|null}
  |{type:"detail";detail:ConversationDetail}
  |{type:"drawer";open:boolean;view?:"history"|"activity"}
  |{type:"event";event:ConversationStreamEvent}

function pendingMessage(id:string,conversationId:string):ChatMessage{return {id,conversation_id:conversationId,role:"assistant",content:"",turn_id:null,status:"streaming",error:null,citations:[],created_at:"",updated_at:""}}

export function reduceConversationEvent(state:ConversationState,event:ConversationStreamEvent):ConversationState{
  if(event.id<=state.lastEventId)return state
  const messageId=event.message_id
  if(!messageId)return {...state,lastEventId:event.id}
  const current=state.messages[messageId]??pendingMessage(messageId,event.conversation_id)
  let next=current
  if(event.type==="answer-progress")next={...current,status:"streaming",progress_phase:event.payload.phase==="reading"?"reading":"reasoning"}
  else if(event.type==="answer-completed")next={...current,status:"completed",content:String(event.payload.answer_markdown??""),citations:(event.payload.citations as MessageCitation[]|undefined)??[],progress_phase:undefined}
  else if(event.type==="answer-failed")next={...current,status:"failed",error:String(event.payload.message??"回答失败"),progress_phase:undefined}
  else if(event.type==="answer-cancelled")next={...current,status:"cancelled",progress_phase:undefined}
  else if(event.type==="message-created")next={...current,role:(event.payload.role as ChatMessage["role"])??"user",content:String(event.payload.content??""),status:"completed"}
  const exists=state.messageOrder.includes(messageId)
  return {...state,lastEventId:event.id,messages:{...state.messages,[messageId]:next},messageOrder:exists?state.messageOrder:[...state.messageOrder,messageId]}
}

export function conversationReducer(state:ConversationState,action:ConversationAction):ConversationState{
  if(action.type==="conversations")return {...state,conversations:action.items}
  if(action.type==="active")return {...state,activeConversationId:action.id,scopes:[],messages:{},messageOrder:[],lastEventId:0,drawerOpen:false}
  if(action.type==="drawer")return {...state,drawerOpen:action.open,drawerView:action.view??state.drawerView}
  if(action.type==="event")return reduceConversationEvent(state,action.event)
  const messages=Object.fromEntries(action.detail.messages.map(message=>[message.id,message]))
  return {...state,activeConversationId:action.detail.conversation.id,scopes:action.detail.scopes,messages,messageOrder:action.detail.messages.map(message=>message.id)}
}

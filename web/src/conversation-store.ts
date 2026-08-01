import type { CandidateCitation, ChatMessage, CodexGoal, CodexPlanStep, CodexRunSettings, CodexWorkItem, Conversation, ConversationDetail, ConversationScope, ConversationStreamEvent, MessageCitation, ResearchProgressPhase } from "./types"
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
  goal: CodexGoal|null
}

export const conversationInitialState:ConversationState={conversations:[],activeConversationId:null,activeSettings:null,scopes:[],messages:{},messageOrder:[],drawerOpen:false,drawerView:"history",lastEventId:0,pendingSwitch:null,goal:null}

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
  |{type:"goal-loaded";conversationId:string;goal:CodexGoal|null}
  |{type:"event";event:ConversationStreamEvent}

function pendingMessage(id:string,conversationId:string):ChatMessage{return {id,conversation_id:conversationId,role:"assistant",content:"",live_content:"",turn_id:null,status:"streaming",error:null,research_mode:"auto",tool_preferences:[],citations:[],candidate_citations:[],created_at:"",updated_at:""}}

const researchProgressPhases:ResearchProgressPhase[]=["research-planning","research-searching","research-deduplicating","research-inspecting-abstract","research-fetching-fulltext","research-saving-candidates","research-partial"]
function progressPhase(value:unknown):ChatMessage["progress_phase"]{return value==="reading"||value==="reasoning"||value==="tool"||value==="answering"||researchProgressPhases.includes(value as ResearchProgressPhase)?value as ChatMessage["progress_phase"]:undefined}

export function reduceConversationEvent(state:ConversationState,event:ConversationStreamEvent):ConversationState{
  if(state.activeConversationId&&event.conversation_id!==state.activeConversationId)return state
  if(event.id<=state.lastEventId)return state
  if(event.type==="goal-updated")return {...state,lastEventId:event.id,goal:event.payload as unknown as CodexGoal}
  if(event.type==="goal-cleared")return {...state,lastEventId:event.id,goal:null}
  const messageId=event.message_id
  if(!messageId)return {...state,lastEventId:event.id}
  const current=state.messages[messageId]??pendingMessage(messageId,event.conversation_id)
  let next=current
  if(event.type==="answer-queued")next={...current,status:"queued"}
  else if(event.type==="answer-started")next={...current,status:"running",progress_phase:"reasoning",progress_label:"Codex 已开始处理问题…"}
  else if(event.type==="answer-progress")next={...current,status:"streaming",progress_phase:progressPhase(event.payload.phase)??"reasoning",progress_label:String(event.payload.label??"")||undefined}
  else if(event.type==="answer-delta")next={...current,status:"streaming",live_content:`${current.live_content??""}${String(event.payload.text??"")}`,progress_phase:"answering",progress_label:"Codex 正在生成回答…"}
  else if(event.type==="work-summary-delta"||event.type==="work-summary-part"){
    const itemId=String(event.payload.item_id??"")
    const summaryIndex=Number(event.payload.summary_index??0)
    const summaries=[...(current.worklog?.summaries??[])]
    const index=summaries.findIndex(item=>item.item_id===itemId&&item.summary_index===summaryIndex)
    const text=event.type==="work-summary-delta"?String(event.payload.text??""):""
    if(index>=0)summaries[index]={...summaries[index],text:`${summaries[index].text}${text}`}
    else summaries.push({item_id:itemId,summary_index:summaryIndex,text})
    next={...current,status:"streaming",worklog:{summaries,plan:current.worklog?.plan,items:current.worklog?.items??{}}}
  }
  else if(event.type==="plan-updated"){
    const steps=(event.payload.plan as CodexPlanStep[]|undefined)??[]
    const explanation=typeof event.payload.explanation==="string"?event.payload.explanation:current.worklog?.plan?.explanation
    next={...current,status:"streaming",worklog:{summaries:current.worklog?.summaries??[],plan:{...(explanation?{explanation}:{}),steps},items:current.worklog?.items??{}}}
  }
  else if(event.type==="work-item-updated"){
    const itemId=String(event.payload.item_id??"")
    const item:CodexWorkItem={item_id:itemId,item_type:String(event.payload.item_type??"work"),label:String(event.payload.label??"Codex 工作"),status:String(event.payload.status??"inProgress")}
    next={...current,status:"streaming",worklog:{summaries:current.worklog?.summaries??[],plan:current.worklog?.plan,items:{...(current.worklog?.items??{}),[itemId]:item}}}
  }
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
  const sameConversation=state.activeConversationId===detail.conversation.id
  const messages=Object.fromEntries(detail.messages.map(message=>[message.id,sameConversation&&state.messages[message.id]?.worklog?{...message,worklog:state.messages[message.id].worklog}:message]))
  const {model,reasoning_effort,service_tier}=detail.conversation
  const activeSettings=model&&reasoning_effort?{model,reasoning_effort,service_tier}:null
  const lastEventId=sameConversation?state.lastEventId:0
  const goal=sameConversation?state.goal:null
  return {...state,activeConversationId:detail.conversation.id,activeSettings,scopes:detail.scopes,messages,messageOrder:detail.messages.map(message=>message.id),lastEventId,goal}
}

export function conversationReducer(state:ConversationState,action:ConversationAction):ConversationState{
  if(action.type==="conversations")return {...state,conversations:action.items}
  if(action.type==="active"){
    if(action.id===state.activeConversationId)return state
    return {...state,activeConversationId:action.id,activeSettings:null,scopes:[],messages:{},messageOrder:[],lastEventId:0,drawerOpen:false,pendingSwitch:null,goal:null}
  }
  if(action.type==="switch-start")return {...state,pendingSwitch:{requestId:action.requestId,conversationId:action.conversationId,targetSelection:null,status:"loading"}}
  if(action.type==="switch-resolved"){
    if(state.pendingSwitch?.requestId!==action.requestId||state.pendingSwitch.conversationId!==action.detail.conversation.id)return state
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
  if(action.type==="goal-loaded")return state.activeConversationId===action.conversationId?{...state,goal:action.goal}:state
  if(action.type==="event")return reduceConversationEvent(state,action.event)
  return {...installDetail(state,action.detail),pendingSwitch:null}
}

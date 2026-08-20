import { Archive, ArchiveRestore, Trash2 } from "lucide-react"
import type { Conversation } from "./types"

export type ConversationHistoryView="active"|"archived"

interface ConversationHistoryProps {
  view:ConversationHistoryView
  active:Conversation[]
  archived:Conversation[]
  activeConversationId:string|null
  busyId:string|null
  onView:(view:ConversationHistoryView)=>void
  onOpen:(id:string)=>void
  onArchive:(id:string)=>void
  onRestore:(id:string)=>void
  onDelete:(id:string)=>void
}

const updatedAt=(conversation:Conversation)=>new Date(conversation.updated_at).toLocaleString()

export function ConversationHistory({view,active,archived,activeConversationId,busyId,onView,onOpen,onArchive,onRestore,onDelete}:ConversationHistoryProps){
  const items=view==="active"?active:archived
  return <div className="conversation-history">
    <div className="conversation-history-tabs" role="tablist" aria-label="对话状态">
      <button type="button" role="tab" aria-selected={view==="active"} className={view==="active"?"active":""} onClick={()=>onView("active")}>当前 <span>{active.length}</span></button>
      <button type="button" role="tab" aria-selected={view==="archived"} className={view==="archived"?"active":""} onClick={()=>onView("archived")}>已归档 <span>{archived.length}</span></button>
    </div>
    <div className="conversation-list">
      {!items.length&&<p className="conversation-list-empty">{view==="active"?"暂无对话":"暂无已归档对话"}</p>}
      {items.map(item=><article className={`conversation-row${item.id===activeConversationId?" active":""}`} key={item.id}>
        {view==="active"
          ?<button type="button" className="conversation-row-main" onClick={()=>onOpen(item.id)}><strong>{item.title}</strong><small className="conversation-row-scope">{item.scope_label??"未标记作用域"}</small><span>{updatedAt(item)}</span></button>
          :<div className="conversation-row-main"><strong>{item.title}</strong><small className="conversation-row-scope">{item.scope_label??"未标记作用域"}</small><span>{updatedAt(item)}</span></div>}
        <div className="conversation-row-actions">
          {view==="active"
            ?<button type="button" aria-label={`归档 ${item.title}`} title="归档" disabled={busyId===item.id} onClick={()=>onArchive(item.id)}><Archive/></button>
            :<><button type="button" aria-label={`恢复 ${item.title}`} title="恢复" disabled={busyId===item.id} onClick={()=>onRestore(item.id)}><ArchiveRestore/></button><button type="button" className="danger" aria-label={`永久删除 ${item.title}`} title="永久删除" disabled={busyId===item.id} onClick={()=>onDelete(item.id)}><Trash2/></button></>}
        </div>
      </article>)}
    </div>
  </div>
}

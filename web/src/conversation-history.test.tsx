import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { ConversationHistory } from "./ConversationHistory"
import type { Conversation } from "./types"

const conversation=(id:string,title:string,archived=false):Conversation=>({
  id,
  title,
  thread_id:`thread-${id}`,
  status:"idle",
  model:"gpt-test",
  reasoning_effort:"medium",
  service_tier:null,
  archived_at:archived?"2026-08-01T00:00:00Z":null,
  created_at:"2026-08-01T00:00:00Z",
  updated_at:"2026-08-01T01:00:00Z",
})

describe("ConversationHistory",()=>{
  it("shows active conversations with an archive action",()=>{
    const html=renderToStaticMarkup(<ConversationHistory view="active" active={[conversation("one","当前对话")]} archived={[]} activeConversationId="one" busyId={null} onView={()=>{}} onOpen={()=>{}} onArchive={()=>{}} onRestore={()=>{}} onDelete={()=>{}}/>)
    expect(html).toContain("当前")
    expect(html).toContain("已归档")
    expect(html).toContain("当前对话")
    expect(html).toContain('aria-label="归档 当前对话"')
    expect(html).not.toContain('aria-label="永久删除 当前对话"')
  })

  it("shows archived conversations with restore and permanent delete actions",()=>{
    const html=renderToStaticMarkup(<ConversationHistory view="archived" active={[]} archived={[conversation("old","旧对话",true)]} activeConversationId={null} busyId={null} onView={()=>{}} onOpen={()=>{}} onArchive={()=>{}} onRestore={()=>{}} onDelete={()=>{}}/>)
    expect(html).toContain("旧对话")
    expect(html).toContain('aria-label="恢复 旧对话"')
    expect(html).toContain('aria-label="永久删除 旧对话"')
    expect(html).not.toContain('aria-label="归档 旧对话"')
  })
})

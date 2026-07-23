import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { Workbench } from "./App"
import type { Dashboard, Task } from "./types"

const task = (id:string,state:string):Task=>({id,kind:"ingest",state,input_json:JSON.stringify({source:id}),paper_id:null,project_id:null,thread_id:null,error:state==="failed"?"失败原因":null,created_at:`2026-07-19T00:00:0${id.length}Z`,updated_at:"2026-07-19T00:00:00Z"})

describe("Workbench intake sections",()=>{
  it("renders active and failed tasks in separate actionable sections",()=>{
    const dashboard:Dashboard={papers:[],projects:[],tasks:[task("active","analyzing"),task("failed","failed"),task("done","done")],inbox:[],trash_count:0,project_memberships:{}}
    const html=renderToStaticMarkup(<Workbench dashboard={dashboard} select={()=>{}} refresh={async()=>{}}/>)
    expect(html).toContain("正在处理")
    expect(html).toContain("最近失败")
    expect(html).toContain("清除失败记录")
    expect(html).toContain('aria-label="取消任务"')
    expect(html).toContain('aria-label="关闭记录"')
    expect(html).not.toContain("task-done")
  })
})

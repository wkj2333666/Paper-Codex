import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { Sidebar, Workbench } from "./App"
import type { Dashboard } from "./types"

const dashboard: Dashboard = { papers: [], projects: [], tasks: [], inbox: [], trash_count: 0, project_memberships: {} }

describe("dedicated briefing navigation", () => {
  it("shows an active top-level briefing entry in the shared sidebar", () => {
    const html = renderToStaticMarkup(<Sidebar dashboard={dashboard} selection={{kind:"briefing"}} select={()=>{}} refresh={async()=>{}} logout={()=>{}} drawerOpen={false} onCollapse={()=>{}} themePreference="light" resolvedTheme="light" onCycleTheme={()=>{}}/>)
    const active = html.match(/<button class="nav-row active" aria-current="page"[^>]*>([\s\S]*?)<\/button>/)?.[1]
    expect(active).toContain("论文晨报")
    expect(active).not.toContain("工作台")
    expect(html.indexOf("工作台")).toBeLessThan(html.indexOf("论文晨报"))
    expect(html.indexOf("论文晨报")).toBeLessThan(html.indexOf("收件箱"))
  })

  it("keeps the briefing form and body out of the workbench", () => {
    const html = renderToStaticMarkup(<Workbench dashboard={dashboard} select={()=>{}} refresh={async()=>{}}/>)
    expect(html).not.toContain("morning-briefing")
    expect(html).not.toContain("生成今日晨报")
    expect(html).toContain("今天想读什么")
  })
})

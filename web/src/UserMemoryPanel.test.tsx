import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { UserMemoryPanel } from "./UserMemoryPanel"

describe("UserMemoryPanel",()=>{
  it("shows only global profile kinds without a current project",()=>{
    const html=renderToStaticMarkup(<UserMemoryPanel projectId={null} onError={()=>{}}/>)
    expect(html).toContain("全局画像")
    expect(html).toContain("当前项目")
    expect(html).toContain("disabled")
    expect(html).toContain("偏好")
    expect(html).toContain("长期兴趣")
    expect(html).not.toContain("未解决概念")
  })

  it("starts from project learning kinds when a project is selected",()=>{
    const html=renderToStaticMarkup(<UserMemoryPanel projectId="project-one" onError={()=>{}}/>)
    expect(html).toContain("研究目标")
    expect(html).toContain("已掌握")
    expect(html).toContain("未解决概念")
    expect(html).not.toContain("长期兴趣")
  })
})

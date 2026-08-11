import { beforeEach, expect, test, vi } from "vitest"
import { api, session, streamEvents } from "./api"

const storage = new Map<string, string>()
Object.defineProperty(globalThis, "localStorage", {
  configurable: true,
  value: {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
  },
})

let capturedHeaders: Headers[]
let capturedRequests: Array<{url:string;method:string;body:BodyInit|null|undefined}>

beforeEach(() => {
  storage.clear()
  capturedHeaders = []
  capturedRequests=[]
  vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
    capturedHeaders.push(new Headers(init?.headers))
    capturedRequests.push({url:String(input),method:init?.method??"GET",body:init?.body})
    return new Response("{}", { status: 200, headers: { "content-type": "application/json" } })
  })
})

function expectDedicatedTokenHeader(headers: Headers) {
  expect(headers.get("x-paper-codex-token")).toBe("test-token")
  expect(headers.has("authorization")).toBe(false)
}

test("API requests use the dedicated application token header", async () => {
  session.set("test-token")
  await api.dashboard()
  expectDedicatedTokenHeader(capturedHeaders[0])
})

test("PDF requests use the dedicated token header", async () => {
  session.set("test-token")
  await api.pdf("paper-1")
  expectDedicatedTokenHeader(capturedHeaders[0])
})

test("event streams use the dedicated token header", async () => {
  session.set("test-token")
  await streamEvents(0, () => undefined, new AbortController().signal)
  expectDedicatedTokenHeader(capturedHeaders[0])
})

test("project lifecycle, trash, and graph methods use encoded scoped endpoints", async()=>{
  session.set("test-token")
  await api.createProject("子项目","目标","parent-1")
  await api.updateProject("child-1",{name:"新名称",purpose:"新目标",parent_id:null})
  await api.removePaper("child-1","doi:10.1/example")
  await api.trashPaper("doi:10.1/example")
  await api.restorePaper("doi:10.1/example")
  await api.graph({project_id:"child-1",kinds:["paper","method"],include_hypotheses:false})
  expect(capturedRequests.map(request=>[request.method,request.url])).toEqual([
    ["POST","/api/projects"],
    ["PATCH","/api/projects/child-1"],
    ["DELETE","/api/projects/child-1/papers/doi%3A10.1%2Fexample"],
    ["DELETE","/api/paper?id=doi%3A10.1%2Fexample"],
    ["POST","/api/paper/restore?id=doi%3A10.1%2Fexample"],
    ["GET","/api/graph?project_id=child-1&kinds=paper%2Cmethod&include_hypotheses=false"],
  ])
})

test("project README reads and saves with an expected revision", async()=>{
  await api.projectReadme("project/one")
  await api.saveProjectReadme("project/one",{markdown:"# Updated",expected_revision:"revision-1"})
  expect(capturedRequests.slice(-2).map(request=>[request.method,request.url])).toEqual([
    ["GET","/api/projects/project%2Fone/readme"],
    ["PUT","/api/projects/project%2Fone/readme"],
  ])
  expect(JSON.parse(String(capturedRequests.at(-1)?.body))).toEqual({
    markdown:"# Updated",
    expected_revision:"revision-1",
  })
})

test("task cancellation and dismissal use separate encoded endpoints", async()=>{
  await api.cancelTask("task/one")
  await api.dismissTask("task/one")
  expect(capturedRequests.map(request=>[request.method,request.url])).toEqual([
    ["POST","/api/tasks/task%2Fone/cancel"],
    ["DELETE","/api/tasks/task%2Fone"],
  ])
})

test("Codex capabilities and conversation settings use the conversation API", async()=>{
  await api.codexCapabilities()
  await api.createConversation("设置对话", [], {model:"gpt-test", reasoning_effort:"high", service_tier:"priority"})
  await api.updateConversation("conversation-1", {settings:{model:"gpt-test", reasoning_effort:"low", service_tier:null}})
  expect(capturedRequests.map(request=>[request.method,request.url])).toEqual([
    ["GET","/api/codex/capabilities"],
    ["POST","/api/conversations"],
    ["PATCH","/api/conversations/conversation-1"],
  ])
  expect(JSON.parse(String(capturedRequests[1].body))).toMatchObject({settings:{model:"gpt-test", reasoning_effort:"high", service_tier:"priority"}})
  expect(JSON.parse(String(capturedRequests[2].body))).toMatchObject({settings:{model:"gpt-test", reasoning_effort:"low", service_tier:null}})
})

test("conversation deletion uses the encoded conversation endpoint", async()=>{
  await api.deleteConversation("conversation/archived")
  expect(capturedRequests.at(-1)).toMatchObject({
    method:"DELETE",
    url:"/api/conversations/conversation%2Farchived",
  })
})

test("native conversation goals use the dedicated goal endpoint", async()=>{
  await api.conversationGoal("conversation/one")
  await api.setConversationGoal("conversation/one", {objective:"完成综述",status:"active",token_budget:40000})
  await api.clearConversationGoal("conversation/one")
  expect(capturedRequests.slice(-3).map(request=>[request.method,request.url])).toEqual([
    ["GET","/api/conversations/conversation%2Fone/goal"],
    ["PUT","/api/conversations/conversation%2Fone/goal"],
    ["DELETE","/api/conversations/conversation%2Fone/goal"],
  ])
  expect(JSON.parse(String(capturedRequests.at(-2)?.body))).toEqual({
    objective:"完成综述",status:"active",token_budget:40000,
  })
})

test("conversation messages send explicit project research mode", async()=>{
  await api.sendConversationMessage("conversation/1","查找相关工作","explicit",{
    name:"paper-research",
    path:"/workspace/.codex/skills/paper-research/SKILL.md",
  })
  expect(capturedRequests.at(-1)).toMatchObject({
    method:"POST",
    url:"/api/conversations/conversation%2F1/messages",
  })
  expect(JSON.parse(String(capturedRequests.at(-1)?.body))).toEqual({
    content:"查找相关工作",
    research_mode:"explicit",
    skill:{
      name:"paper-research",
      path:"/workspace/.codex/skills/paper-research/SKILL.md",
    },
  })
})

test("Codex integrations support an explicit refresh", async()=>{
  await api.codexIntegrations(true)
  expect(capturedRequests.at(-1)).toMatchObject({
    method:"GET",
    url:"/api/codex/integrations?refresh=true",
  })
})

test("project literature methods encode both project and work identifiers", async()=>{
  await api.projectCandidates("project/one",true)
  await api.updateCandidate("project/one","doi:10.1/work",{status:"dismissed"})
  await api.removeCandidate("project/one","doi:10.1/work")
  await api.importCandidate("project/one","doi:10.1/work")
  await api.projectLiteratureSearches("project/one")
  await api.literatureSearch("project/one","run/one")
  expect(capturedRequests.map(request=>[request.method,request.url])).toEqual([
    ["GET","/api/projects/project%2Fone/candidates?include_dismissed=true"],
    ["PATCH","/api/projects/project%2Fone/candidates/doi%3A10.1%2Fwork"],
    ["DELETE","/api/projects/project%2Fone/candidates/doi%3A10.1%2Fwork"],
    ["POST","/api/projects/project%2Fone/candidates/doi%3A10.1%2Fwork/import"],
    ["GET","/api/projects/project%2Fone/literature-searches"],
    ["GET","/api/projects/project%2Fone/literature-searches/run%2Fone"],
  ])
})

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

test("task cancellation and dismissal use separate encoded endpoints", async()=>{
  await api.cancelTask("task/one")
  await api.dismissTask("task/one")
  expect(capturedRequests.map(request=>[request.method,request.url])).toEqual([
    ["POST","/api/tasks/task%2Fone/cancel"],
    ["DELETE","/api/tasks/task%2Fone"],
  ])
})

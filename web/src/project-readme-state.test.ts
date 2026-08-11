import { describe, expect, it } from "vitest"
import { initialProjectReadmeState, projectReadmeReducer } from "./project-readme-state"

const loaded={markdown:"# Original",revision:"revision-1",updated_at:"2026-08-11T00:00:00Z"}

describe("project README editor state",()=>{
  it("becomes dirty after editing a loaded document",()=>{
    const ready=projectReadmeReducer(initialProjectReadmeState,{type:"loaded",value:loaded})
    const dirty=projectReadmeReducer(ready,{type:"edit",markdown:"# Changed"})
    expect(dirty).toMatchObject({status:"dirty",markdown:"# Changed",revision:"revision-1"})
  })

  it("ignores an acknowledgement from an older save request",()=>{
    const ready=projectReadmeReducer(initialProjectReadmeState,{type:"loaded",value:loaded})
    const dirty=projectReadmeReducer(ready,{type:"edit",markdown:"# Changed"})
    const saving=projectReadmeReducer(dirty,{type:"saving",requestId:2})
    const stale=projectReadmeReducer(saving,{type:"saved",requestId:1,value:{...loaded,revision:"old"}})
    expect(stale).toBe(saving)
  })

  it("keeps newer edits dirty when an in-flight save completes",()=>{
    const ready=projectReadmeReducer(initialProjectReadmeState,{type:"loaded",value:loaded})
    const firstEdit=projectReadmeReducer(ready,{type:"edit",markdown:"# First"})
    const saving=projectReadmeReducer(firstEdit,{type:"saving",requestId:3})
    const secondEdit=projectReadmeReducer(saving,{type:"edit",markdown:"# Second"})
    expect(secondEdit.status).toBe("saving")
    const saved=projectReadmeReducer(secondEdit,{type:"saved",requestId:3,value:{...loaded,markdown:"# First",revision:"revision-2"}})
    expect(saved).toMatchObject({status:"dirty",markdown:"# Second",revision:"revision-2"})
  })

  it("enters a conflict state with the server revision",()=>{
    const ready=projectReadmeReducer(initialProjectReadmeState,{type:"loaded",value:loaded})
    const dirty=projectReadmeReducer(ready,{type:"edit",markdown:"# Local"})
    const saving=projectReadmeReducer(dirty,{type:"saving",requestId:4})
    const conflict=projectReadmeReducer(saving,{type:"conflict",requestId:4,currentRevision:"revision-server"})
    expect(conflict).toMatchObject({status:"conflict",markdown:"# Local",conflictRevision:"revision-server"})
  })

  it("restores a local draft against the revision it was based on",()=>{
    const restored=projectReadmeReducer(initialProjectReadmeState,{type:"loaded",value:loaded,draft:{markdown:"# Local draft",baseRevision:"revision-1"}})
    expect(restored).toMatchObject({status:"dirty",markdown:"# Local draft",savedMarkdown:"# Original",revision:"revision-1"})
  })

  it("keeps a stale local draft as an explicit conflict",()=>{
    const restored=projectReadmeReducer(initialProjectReadmeState,{type:"loaded",value:loaded,draft:{markdown:"# Offline draft",baseRevision:"older-revision"}})
    expect(restored).toMatchObject({status:"conflict",markdown:"# Offline draft",savedMarkdown:"# Original",revision:"revision-1",conflictRevision:"revision-1"})
  })
})

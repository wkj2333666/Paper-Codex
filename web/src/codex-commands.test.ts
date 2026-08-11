import { describe, expect, it } from "vitest"
import { applyCodexCommand, codexCommandCompletion } from "./codex-commands"

describe("Codex slash commands",()=>{
  it("completes native goal and compact commands at the start of the composer",()=>{
    expect(codexCommandCompletion("/go",3)?.items.map(item=>item.name)).toEqual(["goal"])
    expect(codexCommandCompletion("/compact",8)?.items.map(item=>item.name)).toEqual(["compact"])
    expect(codexCommandCompletion("讨论 /goal",8)).toBeNull()
  })

  it("applies commands with the right argument spacing",()=>{
    expect(applyCodexCommand("/go",{start:0,end:3},"goal")).toEqual({text:"/goal ",cursor:6})
    expect(applyCodexCommand("/co",{start:0,end:3},"compact")).toEqual({text:"/compact",cursor:8})
  })
})

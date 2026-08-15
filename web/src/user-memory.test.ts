import { describe, expect, it } from "vitest"
import { memoryKindsForScope, memorySourceLabel, memoryUpdatedLabel } from "./user-memory"

describe("user memory presentation",()=>{
  it("separates global profile kinds from project learning kinds",()=>{
    expect(memoryKindsForScope("global")).toEqual(["preference","interest"])
    expect(memoryKindsForScope("project")).toEqual([
      "goal","known_concept","unresolved_concept","terminology","feedback",
    ])
  })

  it("uses user-facing provenance labels and preserves invalid timestamps",()=>{
    expect(memorySourceLabel("explicit_user")).toBe("用户明确记录")
    expect(memorySourceLabel("inferred")).toBe("根据对话推断")
    expect(memoryUpdatedLabel("not-a-date")).toBe("not-a-date")
  })
})

import { describe, expect, it } from "vitest"
import { scopesMatchSelection, selectionForScopes } from "./conversation-scope"
import type { ConversationScope } from "./types"

const scope = (scope_type: ConversationScope["scope_type"], scope_id: string | null): ConversationScope => ({ scope_type, scope_id })

describe("conversation scope", () => {
  it("maps saved scopes back to workspace navigation", () => {
    expect(selectionForScopes([scope("paper", "paper:one")])).toEqual({ kind: "paper", id: "paper:one" })
    expect(selectionForScopes([scope("project", "project-one")])).toEqual({ kind: "project", id: "project-one" })
    expect(selectionForScopes([scope("global", null)])).toEqual({ kind: "workbench" })
    expect(selectionForScopes([scope("paper", null)])).toBeNull()
  })

  it("detects when ordinary page navigation leaves the active conversation scope", () => {
    const paper = [scope("paper", "paper:one")]
    expect(scopesMatchSelection(paper, { kind: "paper", id: "paper:one" })).toBe(true)
    expect(scopesMatchSelection(paper, { kind: "paper", id: "paper:two" })).toBe(false)
    expect(scopesMatchSelection(paper, { kind: "project", id: "project-one" })).toBe(false)
    expect(scopesMatchSelection([scope("global", null)], { kind: "workbench" })).toBe(true)
    expect(scopesMatchSelection([], { kind: "workbench" })).toBe(false)
  })
})

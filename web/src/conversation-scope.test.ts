import { describe, expect, it } from "vitest"
import {
  scopesMatchSelection,
  selectionForScopes,
  selectionsEqual,
  shouldClearConversationForSelection,
} from "./conversation-scope"
import type { ConversationScope } from "./types"

const scope = (scope_type: ConversationScope["scope_type"], scope_id: string | null): ConversationScope => ({ scope_type, scope_id })

describe("conversation scope", () => {
  it("maps saved scopes back to workspace navigation", () => {
    expect(selectionForScopes([
      scope("project", "project-one"),
      scope("paper", "paper:one"),
    ])).toEqual({ kind: "paper", id: "paper:one", projectId: "project-one" })
    expect(selectionForScopes([scope("paper", "paper:one")])).toEqual({ kind: "paper", id: "paper:one" })
    expect(selectionForScopes([scope("project", "project-one")])).toEqual({ kind: "project", id: "project-one", projectId: "project-one" })
    expect(selectionForScopes([scope("global", null)])).toEqual({ kind: "workbench" })
    expect(selectionForScopes([scope("paper", null)])).toBeNull()
  })

  it("detects when ordinary page navigation leaves the active conversation scope", () => {
    const paper = [scope("project", "project-one"), scope("paper", "paper:one")]
    expect(scopesMatchSelection(paper, { kind: "paper", id: "paper:one", projectId: "project-one" })).toBe(true)
    expect(scopesMatchSelection(paper, { kind: "paper", id: "paper:two", projectId: "project-one" })).toBe(false)
    expect(scopesMatchSelection(paper, { kind: "paper", id: "paper:one", projectId: "project-two" })).toBe(false)
    expect(scopesMatchSelection(paper, { kind: "project", id: "project-one" })).toBe(false)
    expect(scopesMatchSelection([scope("global", null)], { kind: "workbench" })).toBe(true)
    expect(scopesMatchSelection([], { kind: "workbench" })).toBe(false)
  })

  it("does not clear a loaded history conversation while its parent selection catches up", () => {
    const paperBScopes = [
      scope("project", "project-one"),
      scope("paper", "paper-b"),
    ]
    const paperASelection = { kind: "paper" as const, id: "paper-a", projectId: "project-one" }
    const paperBSelection = { kind: "paper" as const, id: "paper-b", projectId: "project-one" }

    expect(shouldClearConversationForSelection(
      paperBScopes,
      paperASelection,
      paperBSelection,
    )).toBe(false)
    expect(selectionsEqual(paperBSelection, {
      kind: "paper",
      id: "paper-b",
      projectId: "project-one",
    })).toBe(true)
  })

  it("keeps loading history content protected before the target scope is known", () => {
    const paperScopes = [
      scope("project", "project-one"),
      scope("paper", "paper-a"),
    ]

    expect(shouldClearConversationForSelection(
      paperScopes,
      { kind: "paper", id: "paper-b", projectId: "project-one" },
      null,
    )).toBe(false)
  })

  it("clears a genuine scope mismatch when no history switch is pending", () => {
    const paperScopes = [
      scope("project", "project-one"),
      scope("paper", "paper-a"),
    ]

    expect(shouldClearConversationForSelection(
      paperScopes,
      { kind: "paper", id: "paper-b", projectId: "project-one" },
      undefined,
    )).toBe(true)
  })
})

import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import {
  CodexComposer,
  composerKeyIntent,
  settingsTargetIsOutside,
} from "./CodexComposer"

const capabilities = {
  default: { model: "gpt-5.6-sol", reasoning_effort: "low", service_tier: null },
  models: [
    {
      id: "gpt-5.6-sol",
      display_name: "GPT-5.6-Sol",
      default_reasoning_effort: "low",
      supported_reasoning_efforts: ["low", "medium"],
      supports_fast: true,
    },
    {
      id: "gpt-5.6-terra",
      display_name: "GPT-5.6-Terra",
      default_reasoning_effort: "medium",
      supported_reasoning_efforts: ["low", "medium"],
      supports_fast: true,
    },
  ],
  supports_dynamic_tools: true,
}

describe("CodexComposer", () => {
  it("renders every model supplied by the backend capability catalog", () => {
    const html = renderToStaticMarkup(
      <CodexComposer
        text=""
        placeholder="询问论文"
        busy={false}
        answerRunning={false}
        projectResearchScope={false}
        controlledResearchAvailable={false}
        researchMode="auto"
        capabilities={capabilities}
        integrations={null}
        integrationsLoading={false}
        settings={capabilities.default}
        selectedSkill={null}
        selectedTools={[]}
        onSelectSkill={() => {}}
        onClearSkill={() => {}}
        onToggleTool={() => {}}
        onRequestIntegrations={() => {}}
        onText={() => {}}
        onSubmit={() => {}}
        onCancel={() => {}}
        onResearchMode={() => {}}
        onSettings={() => {}}
      />,
    )

    expect(html).toContain("GPT-5.6-Sol")
    expect(html).toContain("GPT-5.6-Terra")
  })

  it("maps Enter to submit while preserving newline and IME composition", () => {
    expect(
      composerKeyIntent({
        key: "Enter",
        shiftKey: false,
        isComposing: false,
        canSubmit: true,
      }),
    ).toBe("submit")
    expect(
      composerKeyIntent({
        key: "Enter",
        shiftKey: true,
        isComposing: false,
        canSubmit: true,
      }),
    ).toBe("newline")
    expect(
      composerKeyIntent({
        key: "Enter",
        shiftKey: false,
        isComposing: true,
        canSubmit: true,
      }),
    ).toBe("ignore")
  })

  it("does not submit unrelated keys or disabled composer states", () => {
    expect(
      composerKeyIntent({
        key: "a",
        shiftKey: false,
        isComposing: false,
        canSubmit: true,
      }),
    ).toBe("ignore")
    expect(
      composerKeyIntent({
        key: "Enter",
        shiftKey: false,
        isComposing: false,
        canSubmit: false,
      }),
    ).toBe("ignore")
  })

  it("distinguishes settings interactions inside and outside the popover", () => {
    const inside = {} as Node
    const outside = {} as Node
    const container = {
      contains: (target: Node | null) => target === inside,
    }

    expect(settingsTargetIsOutside(container, inside)).toBe(false)
    expect(settingsTargetIsOutside(container, outside)).toBe(true)
    expect(settingsTargetIsOutside(container, null)).toBe(true)
  })
})

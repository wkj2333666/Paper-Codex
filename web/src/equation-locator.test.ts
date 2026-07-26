import { describe, expect, it } from "vitest"
import { equationNumber, locateEquationRegion } from "./equation-locator"
import type { PdfTextItemLike } from "./pdf-highlight-geometry"

const item = (str: string, x: number, baseline: number, width = Math.max(str.length * 5, 8), height = 10): PdfTextItemLike => ({
  str,
  transform: [height, 0, 0, height, x, baseline],
  width,
  height,
})

describe("equationNumber", () => {
  it("parses Chinese and English equation locators", () => {
    expect(equationNumber("第 4 页，§3.2.1，式(5)")).toBe("5")
    expect(equationNumber("Equation (12) in Appendix C")).toBe("12")
    expect(equationNumber("第 4 页，§3.2.1")).toBeNull()
  })
})

describe("locateEquationRegion", () => {
  it("locates a spatial formula whose linearized quote is absent from PDF text", () => {
    const items = [
      item("To balance these two components, we define the Info-Gain", 50, 620, 250),
      item("Sampler objective as:", 50, 605, 92),
      item("J", 120, 570),
      item("IG", 126, 567),
      item("(a", 145, 570),
      item("t", 155, 567),
      item("| z", 162, 570),
      item("t", 179, 567),
      item(")", 184, 570),
      item("=", 205, 570),
      item("IG", 225, 570),
      item("−", 260, 570),
      item("C", 280, 570),
      item("(5)", 330, 570, 16),
      item("We provide further theoretical analysis in Appendix C.", 50, 530, 250),
      item("right-column prose", 360, 570, 120),
    ]

    const region = locateEquationRegion({
      locator: "第 4 页，§3.2.1，式(5)",
      prefix: "To balance these two components, we define the Info-Gain Sampler objective as:",
      suffix: "We provide further theoretical analysis in Appendix C.",
      revision: "r1",
    }, items, 700, 800, "r1")

    expect(region).not.toBeNull()
    expect(region?.left).toBeLessThan(0.2)
    expect((region?.left ?? 0) + (region?.width ?? 1)).toBeLessThan(0.52)
    expect(region?.top).toBeGreaterThan(0.2)
    expect((region?.top ?? 1) + (region?.height ?? 0)).toBeLessThan(0.35)
  })

  it("rejects an ambiguous equation number instead of highlighting the wrong column", () => {
    const items = [
      item("(5)", 330, 570, 16),
      item("(5)", 680, 570, 16),
    ]
    expect(locateEquationRegion({
      locator: "Equation (5)",
      prefix: "",
      suffix: "",
      revision: "r1",
    }, items, 700, 800, "r1")).toBeNull()
  })

  it("rejects coordinates from a stale paper revision", () => {
    expect(locateEquationRegion({
      locator: "式(5)",
      prefix: "",
      suffix: "",
      revision: "old",
    }, [item("(5)", 330, 570, 16)], 700, 800, "new")).toBeNull()
  })
})

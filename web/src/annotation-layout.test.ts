import { describe, expect, it } from "vitest"
import { placeAnnotationCards } from "./annotation-layout"

describe("placeAnnotationCards", () => {
  it("keeps cards inside the gutter and separates overlaps", () => {
    expect(placeAnnotationCards([
      { id: "a", preferredTop: 80, height: 120 },
      { id: "b", preferredTop: 100, height: 100 },
    ], 320, 12)).toEqual([
      { id: "a", top: 80 },
      { id: "b", top: 212 },
    ])
  })

  it("moves a stack upward when it reaches the bottom", () => {
    expect(placeAnnotationCards([
      { id: "a", preferredTop: 260, height: 80 },
      { id: "b", preferredTop: 280, height: 80 },
    ], 360, 8)).toEqual([
      { id: "a", top: 192 },
      { id: "b", top: 280 },
    ])
  })

  it("keeps three collapsed cards separated at their anchors", () => {
    expect(placeAnnotationCards([
      { id: "first", preferredTop: 24, height: 44 },
      { id: "second", preferredTop: 28, height: 44 },
      { id: "third", preferredTop: 34, height: 44 },
    ], 180, 8)).toEqual([
      { id: "first", top: 24 },
      { id: "second", top: 76 },
      { id: "third", top: 128 },
    ])
  })
})

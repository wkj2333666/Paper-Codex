import type { MemoryItem, MemoryKind } from "./types"

export const memoryKindLabels:Record<MemoryKind,string>={
  preference:"偏好",
  interest:"长期兴趣",
  goal:"研究目标",
  known_concept:"已掌握",
  unresolved_concept:"未解决概念",
  terminology:"术语约定",
  feedback:"反馈",
}

const globalKinds:MemoryKind[]=["preference","interest"]
const projectKinds:MemoryKind[]=["goal","known_concept","unresolved_concept","terminology","feedback"]

export const memoryKindsForScope=(scope:"global"|"project"):MemoryKind[]=>
  scope==="global"?[...globalKinds]:[...projectKinds]

export const memorySourceLabel=(source:MemoryItem["source"]):string=>({
  explicit_user:"用户明确记录",
  confirmed:"用户已确认",
  inferred:"根据对话推断",
  imported:"导入",
})[source]

export const memoryConfidenceLabel=(confidence:MemoryItem["confidence"]):string=>({
  high:"高置信度",
  medium:"中等置信度",
  low:"低置信度",
})[confidence]

export function memoryUpdatedLabel(value:string):string{
  const date=new Date(value)
  return Number.isNaN(date.getTime())?value:date.toLocaleString()
}

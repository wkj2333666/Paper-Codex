import type { ConversationScope } from "./types"

export interface CodexSelection { kind: "workbench"|"inbox"|"paper"|"project"|"search"|"graph"|"trash"; id?: string; projectId?: string }

function scopeKey(scope: ConversationScope): string | null {
  if (scope.scope_type === "global") return "global"
  if (!scope.scope_id) return null
  return `${scope.scope_type}:${scope.scope_id}`
}

function projectIdForSelection(selection: CodexSelection): string | undefined {
  if (selection.kind === "project" && selection.id) return selection.id
  return selection.projectId
}

export function selectionForScopes(scopes: ConversationScope[]): CodexSelection | null {
  const projectId=scopes.find(scope=>scope.scope_type==="project")?.scope_id??undefined
  const paperId=scopes.find(scope=>scope.scope_type==="paper")?.scope_id??undefined
  if(paperId)return {kind:"paper",id:paperId,...(projectId?{projectId}:{})}
  if(projectId)return {kind:"project",id:projectId,projectId}
  if(scopes.some(scope=>scope.scope_type==="global"))return {kind:"workbench"}
  return null
}

export function scopesMatchSelection(scopes: ConversationScope[], selection: CodexSelection): boolean {
  const projectId=projectIdForSelection(selection)
  const openPaperId=selection.kind==="paper"?selection.id:undefined
  const savedProjectIds=scopes.filter(scope=>scope.scope_type==="project"&&scope.scope_id).map(scope=>scope.scope_id)
  const savedPaperIds=scopes.filter(scope=>scope.scope_type==="paper"&&scope.scope_id).map(scope=>scope.scope_id)
  if(projectId){
    return savedProjectIds.length===1&&savedProjectIds[0]===projectId&&
      (openPaperId?savedPaperIds.length===1&&savedPaperIds[0]===openPaperId:savedPaperIds.length===0)
  }
  const current=selection.kind==="paper"&&selection.id?`paper:${selection.id}`:"global"
  return scopes.some(scope=>scopeKey(scope)===current)
}

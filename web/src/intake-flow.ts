import type { DirectIntakeResult, IntakeSearchResponse } from "./types"

type IntakeClient={
  intake:(source:string,projectId?:string)=>Promise<DirectIntakeResult>
  searchIntake:(query:string,limit?:number)=>Promise<IntakeSearchResponse>
}

export type IntakeFlowResult=
  | {state:"enqueued";task_id:string}
  | {state:"candidates";response:IntakeSearchResponse}

function requiresSearch(error:unknown):boolean{
  if(!error||typeof error!=="object")return false
  const body=(error as {body?:unknown}).body
  return !!body&&typeof body==="object"&&(body as {code?:unknown}).code==="intake_search_required"
}

export async function routeIntakeSubmission(source:string,projectId:string|undefined,client:IntakeClient):Promise<IntakeFlowResult>{
  try{
    const result=await client.intake(source,projectId)
    return {state:"enqueued",task_id:result.task_id}
  }catch(error){
    if(!requiresSearch(error))throw error
    return {state:"candidates",response:await client.searchIntake(source,12)}
  }
}

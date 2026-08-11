export type CodexCommandName="goal"|"compact"

export interface CodexCommandDefinition {
  name:CodexCommandName
  label:string
  description:string
  acceptsArgument:boolean
}

export interface CodexCommandRange {start:number;end:number}

export interface CodexCommandCompletion extends CodexCommandRange {
  query:string
  items:CodexCommandDefinition[]
}

export const codexCommands:CodexCommandDefinition[]=[
  {name:"goal",label:"设置目标",description:"让 Codex 持续推进一个项目目标",acceptsArgument:true},
  {name:"compact",label:"压缩上下文",description:"使用 Codex 原生 compact 精简当前对话",acceptsArgument:false},
]

export function codexCommandCompletion(text:string,cursor:number):CodexCommandCompletion|null{
  const before=text.slice(0,cursor)
  const match=before.match(/^\/([a-z-]*)$/i)
  if(!match)return null
  const query=match[1].toLowerCase()
  return {start:0,end:cursor,query,items:codexCommands.filter(item=>item.name.startsWith(query))}
}

export function applyCodexCommand(text:string,range:CodexCommandRange,name:CodexCommandName){
  const command=codexCommands.find(item=>item.name===name)!
  const replacement=`/${name}${command.acceptsArgument?" ":""}`
  const next=`${text.slice(0,range.start)}${replacement}${text.slice(range.end)}`
  return {text:next,cursor:range.start+replacement.length}
}

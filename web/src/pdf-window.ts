export function visiblePageWindow({pageCount,firstVisible,lastVisible,overscan}:{pageCount:number;firstVisible:number;lastVisible:number;overscan:number}):number[]{
  if(pageCount<=0)return []
  const start=Math.max(1,Math.min(firstVisible,lastVisible)-Math.max(0,overscan))
  const end=Math.min(pageCount,Math.max(firstVisible,lastVisible)+Math.max(0,overscan))
  return Array.from({length:end-start+1},(_,index)=>start+index)
}

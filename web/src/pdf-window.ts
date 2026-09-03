export type VisiblePageRange = {first:number;last:number}
export type PdfViewportRect = {left:number;top:number;width:number;height:number}
export type PdfZoomLayout = {viewport:PdfViewportRect;pages:Array<{page:number;rect:PdfViewportRect}>}
export type PdfZoomAnchor = {page:number;xRatio:number;yRatio:number;viewportX:number;viewportY:number}

const EMPTY_PAGE_ITEMS: never[]=[]

export function visiblePageWindow({pageCount,firstVisible,lastVisible,overscan}:{pageCount:number;firstVisible:number;lastVisible:number;overscan:number}):number[]{
  if(pageCount<=0)return []
  const start=Math.max(1,Math.min(firstVisible,lastVisible)-Math.max(0,overscan))
  const end=Math.min(pageCount,Math.max(firstVisible,lastVisible)+Math.max(0,overscan))
  return Array.from({length:end-start+1},(_,index)=>start+index)
}

export function stableVisiblePageRange(current:VisiblePageRange,next:VisiblePageRange):VisiblePageRange{
  return current.first===next.first&&current.last===next.last?current:next
}

export function pageItemsForNumber<T>(itemsByPage:ReadonlyMap<number,T[]>,pageNumber:number):T[]{
  return itemsByPage.get(pageNumber)??EMPTY_PAGE_ITEMS
}

export function capturePdfZoomAnchor(layout:PdfZoomLayout):PdfZoomAnchor|null{
  const centerX=layout.viewport.left+layout.viewport.width/2
  const centerY=layout.viewport.top+layout.viewport.height/2
  const pages=layout.pages.filter(({rect})=>rect.width>0&&rect.height>0)
  const page=pages.reduce<(typeof pages)[number]|null>((nearest,candidate)=>{
    if(!nearest)return candidate
    return distanceToRect(centerX,centerY,candidate.rect)<distanceToRect(centerX,centerY,nearest.rect)?candidate:nearest
  },null)
  if(!page)return null
  return{
    page:page.page,
    xRatio:clampUnit((centerX-page.rect.left)/page.rect.width),
    yRatio:clampUnit((centerY-page.rect.top)/page.rect.height),
    viewportX:centerX-layout.viewport.left,
    viewportY:centerY-layout.viewport.top,
  }
}

export function visiblePdfPageRange(layout:PdfZoomLayout):VisiblePageRange|null{
  const viewportBottom=layout.viewport.top+layout.viewport.height
  const hits=layout.pages
    .filter(({rect})=>rect.top+rect.height>layout.viewport.top&&rect.top<viewportBottom)
    .map(({page})=>page)
  return hits.length?{first:Math.min(...hits),last:Math.max(...hits)}:null
}

export function finishPdfZoom(
  anchor:PdfZoomAnchor|null,
  readLayout:()=>PdfZoomLayout,
  scrollBy:(left:number,top:number)=>void,
):VisiblePageRange|null{
  const layout=readLayout()
  const page=anchor?layout.pages.find(candidate=>candidate.page===anchor.page):null
  if(anchor&&page){
    const anchorX=page.rect.left+page.rect.width*anchor.xRatio
    const anchorY=page.rect.top+page.rect.height*anchor.yRatio
    scrollBy(
      anchorX-(layout.viewport.left+anchor.viewportX),
      anchorY-(layout.viewport.top+anchor.viewportY),
    )
  }
  return visiblePdfPageRange(readLayout())
}

function distanceToRect(x:number,y:number,rect:PdfViewportRect):number{
  const dx=Math.max(rect.left-x,0,x-(rect.left+rect.width))
  const dy=Math.max(rect.top-y,0,y-(rect.top+rect.height))
  return dx*dx+dy*dy
}

function clampUnit(value:number):number{
  return Math.max(0,Math.min(1,value))
}

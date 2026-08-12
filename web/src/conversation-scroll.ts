export const BOTTOM_THRESHOLD_PX = 24

export interface ConversationScrollViewport {
  scrollHeight: number
  scrollTop: number
  clientHeight: number
  scrollTo(options: ScrollToOptions): void
}

export function isConversationAtBottom(
  viewport: Pick<ConversationScrollViewport, "scrollHeight" | "scrollTop" | "clientHeight">,
  threshold = BOTTOM_THRESHOLD_PX,
): boolean {
  const remaining = Math.max(0, viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight)
  return remaining <= threshold
}

export class ConversationScrollController {
  private pinned = true

  constructor(private readonly viewport: () => ConversationScrollViewport | null) {}

  reset(): void {
    this.pinned = true
  }

  handleScroll(): void {
    const viewport = this.viewport()
    if (viewport) this.pinned = isConversationAtBottom(viewport)
  }

  positionInitial(): void {
    const viewport = this.viewport()
    if (!viewport) return
    this.pinned = true
    viewport.scrollTo({ top: viewport.scrollHeight, behavior: "auto" })
  }

  followContent(): void {
    const viewport = this.viewport()
    if (!viewport || !this.pinned) return
    viewport.scrollTo({ top: viewport.scrollHeight, behavior: "smooth" })
  }

  isPinned(): boolean {
    return this.pinned
  }
}

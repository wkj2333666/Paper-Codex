export interface AnnotationCardRequest { id: string; preferredTop: number; height: number }
export interface AnnotationCardPlacement { id: string; top: number }

export function placeAnnotationCards(cards: AnnotationCardRequest[], gutterHeight: number, gap = 12): AnnotationCardPlacement[] {
  if (!cards.length) return []
  const ordered = [...cards].sort((left, right) => left.preferredTop - right.preferredTop)
  const placements = ordered.map((card, index) => ({
    id: card.id,
    top: index === 0 ? Math.max(0, card.preferredTop) : Math.max(card.preferredTop, 0),
  }))
  for (let index = 1; index < placements.length; index += 1) {
    placements[index].top = Math.max(placements[index].top, placements[index - 1].top + ordered[index - 1].height + gap)
  }
  const overflow = placements.at(-1)!.top + ordered.at(-1)!.height - gutterHeight
  if (overflow > 0) placements.forEach(placement => { placement.top = Math.max(0, placement.top - overflow) })
  return placements
}

/** Place measured editing controls inside the usable canvas, below the selection if needed. */
export function toolbarPosition(
  anchor: { x: number; top: number; bottom: number },
  size: { width: number; height: number },
  viewport: { width: number; height: number },
) {
  const margin = 8
  const width = Math.min(size.width, Math.max(0, viewport.width - margin * 2))
  const height = Math.min(size.height, Math.max(0, viewport.height - margin * 2))
  const above = anchor.top - height - margin
  return {
    left: Math.max(margin, Math.min(viewport.width - width - margin, anchor.x - width / 2)),
    top: Math.max(margin, Math.min(viewport.height - height - margin, above >= margin ? above : anchor.bottom + margin)),
  }
}

import { exportToSvg, getCommonBounds } from '@excalidraw/excalidraw'
import { overlayElements, withInheritedInk, type FreeScene } from './freeScene.ts'
import type { Graph } from '../engine/graph.ts'

/** SVG is consumed as an image, never injected as active document markup. */
export async function withSceneOverlay(state: FreeScene, graph?: Graph, base?: Graph, original?: FreeScene | null): Promise<FreeScene> {
  const overlay = overlayElements(state)
  const { elements } = graph ? withInheritedInk({ ...state, elements: overlay.elements }, graph, base, original) : overlay
  const { hidden } = overlay
  if (!elements.length) return { ...state, overlay: hidden.length ? { url: '', x: 0, y: 0, width: 0, height: 0, hidden } : undefined }
  const [x, y, x1, y1] = getCommonBounds(elements)
  const padding = 12
  const svg = await exportToSvg({ elements, files: state.files, appState: { exportBackground: false, exportWithDarkMode: false }, exportPadding: padding })
  const text = new XMLSerializer().serializeToString(svg)
  const bytes = new TextEncoder().encode(text)
  let binary = ''
  for (const byte of bytes) binary += String.fromCharCode(byte)
  return {
    ...state,
    overlay: {
      url: `data:image/svg+xml;base64,${btoa(binary)}`,
      x: x - padding, y: y - padding,
      width: x1 - x + padding * 2, height: y1 - y + padding * 2,
      hidden,
    },
  }
}

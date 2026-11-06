/** Recolor vector ink for the dark board while leaving every embedded image untouched. */
export function darkOverlayUrl(url: string): string {
  const prefix = 'data:image/svg+xml;base64,'
  if (!url.startsWith(prefix)) return url
  const source = new TextDecoder().decode(Uint8Array.from(atob(url.slice(prefix.length)), (ch) => ch.charCodeAt(0)))
  const document = new DOMParser().parseFromString(source, 'image/svg+xml')
  if (document.querySelector('parsererror')) return url
  // Excalidraw dark export filters the entire SVG, then approximately compensates
  // raster <use> images; SVG images remain inverted. Filter only actual vector ink
  // here, retaining original image bytes, colors, transforms and z-order.
  for (const element of document.querySelectorAll('path,rect,circle,ellipse,polygon,polyline,line,text')) {
    if (element.closest('defs,mask,clipPath,symbol')) continue
    element.setAttribute('filter', 'invert(93%) hue-rotate(180deg)')
  }
  const bytes = new TextEncoder().encode(new XMLSerializer().serializeToString(document))
  let binary = ''
  for (const byte of bytes) binary += String.fromCharCode(byte)
  return prefix + btoa(binary)
}

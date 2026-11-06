import { type ReactNode, useLayoutEffect, useRef, useState } from 'react'
import { toolbarPosition } from './toolbar.ts'

export default function SelectionToolbar({ anchor, viewport, children }: {
  anchor: { x: number; top: number; bottom: number }
  viewport: { width: number; height: number }
  children: ReactNode
}) {
  const ref = useRef<HTMLDivElement>(null)
  const [size, setSize] = useState({ width: 0, height: 0 })
  useLayoutEffect(() => {
    const el = ref.current!
    const measure = () => setSize({ width: el.offsetWidth, height: el.offsetHeight })
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(el)
    return () => observer.disconnect()
  }, [])
  return <div
    ref={ref}
    className="ink-toolbar"
    style={{ ...toolbarPosition(anchor, size, viewport), maxWidth: Math.max(0, viewport.width - 16), maxHeight: Math.max(0, viewport.height - 16), visibility: size.width ? undefined : 'hidden' }}
    onPointerDown={(e) => e.stopPropagation()}
  >{children}</div>
}

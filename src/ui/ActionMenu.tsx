import { type ReactNode, type RefObject, useLayoutEffect, useRef } from 'react'
import { createPortal } from 'react-dom'

export default function ActionMenu({ label, trigger, onClose, className = '', children }: {
  label: string
  trigger: RefObject<HTMLButtonElement | null>
  onClose: () => void
  className?: string
  children: ReactNode
}) {
  const ref = useRef<HTMLDivElement>(null)
  const close = useRef(onClose)
  close.current = onClose
  useLayoutEffect(() => {
    const menu = ref.current!
    const position = () => {
      if (!trigger.current) return
      const box = trigger.current.getBoundingClientRect()
      menu.style.maxHeight = `${Math.max(48, innerHeight - 24)}px`
      const width = menu.offsetWidth, height = menu.offsetHeight
      menu.style.left = `${Math.max(12, Math.min(box.right - width, innerWidth - width - 12))}px`
      menu.style.top = `${Math.max(12, Math.min(box.bottom + 6, innerHeight - height - 12))}px`
    }
    position()
    menu.querySelector<HTMLElement>('[role^="menuitem"]:not(:disabled)')?.focus({ preventScroll: true })
    const outside = (event: PointerEvent) => {
      if (!menu.contains(event.target as Node) && !trigger.current?.contains(event.target as Node)) close.current()
    }
    const scroll = (event: Event) => { if (!menu.contains(event.target as Node)) position() }
    const size = new ResizeObserver(position)
    size.observe(menu)
    document.addEventListener('pointerdown', outside)
    addEventListener('resize', position)
    addEventListener('scroll', scroll, true)
    return () => {
      size.disconnect()
      document.removeEventListener('pointerdown', outside)
      removeEventListener('resize', position)
      removeEventListener('scroll', scroll, true)
    }
  }, [trigger])
  return createPortal(<div ref={ref} role="menu" aria-label={label} className={`menu action-menu ${className}`} onBlur={(event) => {
    if (!event.currentTarget.contains(event.relatedTarget as Node) && !trigger.current?.contains(event.relatedTarget as Node)) onClose()
  }} onKeyDown={(event) => {
    if (event.nativeEvent.isComposing) return
    if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
      trigger.current?.focus()
      onClose()
      return
    }
    if ((event.target as Element).closest('input, textarea, select')) return
    const items = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('[role^="menuitem"]:not(:disabled)')]
    const current = items.indexOf(document.activeElement as HTMLButtonElement)
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : event.key === 'ArrowDown' ? (current + 1) % items.length : event.key === 'ArrowUp' ? (current + items.length - 1) % items.length : null
    if (next !== null) { event.preventDefault(); items[next]?.focus(); items[next]?.scrollIntoView({ block: 'nearest' }) }
  }}>{children}</div>, document.body)
}

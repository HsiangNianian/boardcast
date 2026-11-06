import { useLayoutEffect, useRef, type ReactNode } from 'react'
import './dialog.css'

const controls = (dialog: HTMLDialogElement) => [...dialog.querySelectorAll<HTMLElement>(
  'button, input, textarea, select, a[href], summary, [tabindex], [contenteditable="true"]',
)].filter((element) => !element.matches(':disabled') && element.tabIndex >= 0 && element.getClientRects().length > 0)

export default function Dialog({ label, onClose, dismissible = true, role = 'dialog', className = '', children }: {
  label: string
  onClose: () => void
  dismissible?: boolean
  role?: 'dialog' | 'alertdialog'
  className?: string
  children: ReactNode
}) {
  const ref = useRef<HTMLDialogElement>(null)
  useLayoutEffect(() => {
    const dialog = ref.current!
    const previous = document.activeElement
    dialog.showModal()
    return () => {
      dialog.close()
      // A parent and its nested dialog can unmount together; restore after both release inertness.
      queueMicrotask(() => { if (previous instanceof HTMLElement && previous.isConnected) previous.focus() })
    }
  }, [])

  return <dialog
    ref={ref}
    className={`modal ${className}`}
    role={role}
    aria-modal="true"
    aria-label={label}
    onCancel={(event) => { event.preventDefault(); if (dismissible) onClose() }}
    onClick={(event) => { if (event.target === event.currentTarget && dismissible) onClose() }}
    onKeyDown={(event) => {
      // Keep application shortcuts outside both this dialog and any nested native dialog.
      event.stopPropagation()
      if (event.key === 'Escape') {
        event.preventDefault()
        if (!event.nativeEvent.isComposing && dismissible) onClose()
      }
      if (event.key !== 'Tab') return
      const items = controls(event.currentTarget)
      const first = items[0], last = items.at(-1)
      if (!first) { event.preventDefault(); return }
      if (!event.currentTarget.contains(document.activeElement) || document.activeElement === (event.shiftKey ? first : last)) {
        event.preventDefault()
        const next = event.shiftKey ? last : first
        next?.focus()
      }
    }}
  >{children}</dialog>
}

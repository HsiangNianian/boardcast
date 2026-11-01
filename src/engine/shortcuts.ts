// 应用里的快捷键。铅笔模式由 Excalidraw 自己处理，这些只在手写模式生效；焦点在输入框里时不抢键。

export type Action = 'prevPage' | 'nextPage' | 'newPage' | 'import' | 'replay' | 'summary' | 'help'

interface Key {
  key: string
  ctrlKey: boolean
  metaKey: boolean
  shiftKey: boolean
  altKey: boolean
}

export function actionFor(e: Key): Action | null {
  if (e.altKey) return null
  const mod = e.ctrlKey || e.metaKey
  const k = e.key.length === 1 ? e.key.toLowerCase() : e.key
  if (!mod) {
    if (k === 'PageUp') return 'prevPage'
    if (k === 'PageDown') return 'nextPage'
    if (e.key === '?') return 'help'
    return null
  }
  if (e.shiftKey) {
    if (k === 'n') return 'newPage'
    if (k === 'r') return 'replay'
    if (k === 's') return 'summary'
    return null
  }
  if (k === '[') return 'prevPage'
  if (k === ']') return 'nextPage'
  if (k === 'o') return 'import'
  return null
}

/** 把一次按键写成全局快捷键的写法（Ctrl+Shift+Space）；只按了修饰键，或者没带修饰键，返回 null */
export function accelFrom(e: Key & { code: string }): string | null {
  const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super'].filter(Boolean) as string[]
  if (!mods.length) return null
  const code = e.code
  const main = /^Key[A-Z]$/.test(code)
    ? code.slice(3)
    : /^Digit\d$/.test(code)
      ? code.slice(5)
      : /^F\d{1,2}$/.test(code) || code === 'Space'
        ? code
        : null
  return main ? [...mods, main].join('+') : null
}

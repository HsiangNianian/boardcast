// Interface language: follows the system by default, can be pinned in Settings (stored per device).
// English is the fallback for any system language we don't have a dictionary for.
// React components read strings with `useT()`; plain modules call `msg()`.

import { useSyncExternalStore } from 'react'
import { type Messages, en } from './en.ts'
import { zh } from './zh.ts'

export type Lang = 'en' | 'zh'
export type LangChoice = Lang | 'system'
export type { Messages }

export const LANGS: Lang[] = ['en', 'zh']
/** Each language's own name, shown in the picker regardless of the current language */
export const LANG_NAMES: Record<Lang, string> = { en: 'English', zh: '中文' }

const dicts: Record<Lang, Messages> = { en, zh }
const KEY = 'boardcast.lang'

/** First preferred language we have a dictionary for; Chinese (any script, plus Cantonese) maps to zh */
export function systemLang(prefs: readonly string[]): Lang {
  for (const p of prefs) {
    if (/^(zh|yue)\b/i.test(p)) return 'zh'
    if (/^en\b/i.test(p)) return 'en'
  }
  return 'en'
}

function preferred(): readonly string[] {
  if (typeof navigator === 'undefined') return []
  return navigator.languages?.length ? navigator.languages : [navigator.language]
}

function stored(): LangChoice {
  // Outside a browser (tests, the eval) there is nothing stored; Node's own experimental
  // localStorage would only print a warning
  if (typeof window === 'undefined') return 'system'
  try {
    const v = localStorage.getItem(KEY)
    return v === 'en' || v === 'zh' ? v : 'system'
  } catch {
    return 'system'
  }
}

let choice: LangChoice = stored()
let lang: Lang = choice === 'system' ? systemLang(preferred()) : choice
const listeners = new Set<() => void>()

function apply() {
  const next = choice === 'system' ? systemLang(preferred()) : choice
  if (typeof document !== 'undefined') document.documentElement.lang = next === 'zh' ? 'zh-CN' : 'en'
  if (next === lang) return
  lang = next
  for (const f of listeners) f()
}

apply()
if (typeof window !== 'undefined') window.addEventListener('languagechange', apply)

export function currentLang(): Lang {
  return lang
}

export function langChoice(): LangChoice {
  return choice
}

export function setLangChoice(c: LangChoice) {
  choice = c
  if (typeof window !== 'undefined')
    try {
      if (c === 'system') localStorage.removeItem(KEY)
      else localStorage.setItem(KEY, c)
    } catch {
      /* not persisted; still applies for this session */
    }
  apply()
  // The picker shows the choice, which can change without the resolved language changing
  for (const f of listeners) f()
}

function subscribe(f: () => void) {
  listeners.add(f)
  return () => listeners.delete(f)
}

/** Strings in the current language, for code outside React */
export function msg(): Messages {
  return dicts[lang]
}

/** Strings in the current language; re-renders when it changes */
export function useT(): Messages {
  return useSyncExternalStore(subscribe, msg)
}

/** The Settings picker's value: a pinned language or 'system' */
export function useLangChoice(): LangChoice {
  return useSyncExternalStore(subscribe, langChoice)
}

import { createI18n, useI18n } from 'vue-i18n'

import enUS from './en-US'
import zhCN, { type MessageSchema } from './zh-CN'

export type Locale = 'zh-CN' | 'en-US'

/**
 * Language names in their own language, whatever the current one is, so that someone who cannot
 * read the current language can still find theirs (IXD-36).
 */
export const LOCALE_NAMES: Record<Locale, string> = {
  'zh-CN': '简体中文',
  'en-US': 'English',
}

/** English entries that are not translated yet fall back to Chinese (phase 5b, task 8.4c). */
export const i18n = createI18n<[MessageSchema], Locale, false>({
  legacy: false,
  locale: 'zh-CN',
  fallbackLocale: 'zh-CN',
  messages: {
    'zh-CN': zhCN,
    'en-US': enUS as MessageSchema,
  },
  // Untranslated English is expected until 8.4c; a key missing from zh-CN still warns.
  fallbackWarn: false,
})

/** Dotted paths of every message, e.g. `'settings.look.title'`. */
type Leaves<T, P extends string = ''> = {
  [K in keyof T & string]: T[K] extends string ? `${P}${K}` : Leaves<T[K], `${P}${K}.`>
}[keyof T & string]

export type MessageKey = Leaves<MessageSchema>

/** Values for named placeholders such as `{server}`. */
export type MessageParams = Record<string, string | number>

/**
 * `t()` for components, restricted to keys that exist in zh-CN.ts. vue-i18n's own `t` accepts
 * any string, so a typo would only show up at run time.
 */
export function useT() {
  const { t } = useI18n()
  return (key: MessageKey, params: MessageParams = {}) => t(key, params)
}

/** The same outside components (stores, composables); follows the current language. */
export function translate(key: MessageKey, params: MessageParams = {}) {
  return i18n.global.t(key, params)
}

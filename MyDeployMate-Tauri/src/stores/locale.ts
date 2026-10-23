import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

import { i18n, type Locale } from '@/i18n'

/** Same key as the inline script in index.html, which sets `<html lang>` before Vue mounts. */
export const LOCALE_KEY = 'mydeploymate.locale'

export type LocalePreference = 'system' | Locale

export function parseLocalePreference(raw: string | null): LocalePreference {
  return raw === 'zh-CN' || raw === 'en-US' ? raw : 'system'
}

/** NFR-05: follow the Windows display language, Chinese for any `zh` variant and English otherwise. */
export function resolveLocale(preference: LocalePreference, systemLanguage: string): Locale {
  if (preference !== 'system') return preference
  return systemLanguage.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US'
}

function readPreference(): LocalePreference {
  try {
    return parseLocalePreference(localStorage.getItem(LOCALE_KEY))
  } catch {
    return 'system'
  }
}

/**
 * Interface language (NFR-05, IXD-36). A per-machine preference like the theme: it is not part
 * of the workspace and is not exported. Switching re-renders in place; drafts, open dialogs and
 * a running pipeline are untouched.
 */
export const useLocaleStore = defineStore('locale', () => {
  const preference = ref<LocalePreference>(readPreference())
  const systemLanguage = ref(navigator.language)
  window.addEventListener('languagechange', () => (systemLanguage.value = navigator.language))

  watch(
    [preference, systemLanguage],
    () => {
      const locale = resolveLocale(preference.value, systemLanguage.value)
      i18n.global.locale.value = locale
      // Also picks the font stack (UI-07).
      document.documentElement.lang = locale
    },
    { immediate: true },
  )

  watch(preference, (value) => {
    try {
      localStorage.setItem(LOCALE_KEY, value)
    } catch {
      // Storage unavailable: the choice lasts until the app closes.
    }
  })

  return { preference }
})

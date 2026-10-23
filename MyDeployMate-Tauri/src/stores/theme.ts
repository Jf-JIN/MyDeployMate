import { getCurrentWindow } from '@tauri-apps/api/window'
import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

/** Same key as the inline script in index.html, which applies the theme before Vue mounts. */
export const THEME_KEY = 'mydeploymate.theme'

export type ThemePreference = 'system' | 'light' | 'dark'
export type Theme = 'light' | 'dark'

export function parsePreference(raw: string | null): ThemePreference {
  return raw === 'light' || raw === 'dark' ? raw : 'system'
}

export function resolveTheme(preference: ThemePreference, systemDark: boolean): Theme {
  if (preference === 'system') return systemDark ? 'dark' : 'light'
  return preference
}

function readPreference(): ThemePreference {
  try {
    return parsePreference(localStorage.getItem(THEME_KEY))
  } catch {
    return 'system'
  }
}

/**
 * Light, dark or follow Windows (UI-02). A per-machine preference like the session: it is not
 * part of the workspace and is not exported.
 */
export const useThemeStore = defineStore('theme', () => {
  const preference = ref<ThemePreference>(readPreference())
  const media = window.matchMedia('(prefers-color-scheme: dark)')
  const systemDark = ref(media.matches)
  media.addEventListener('change', (e) => (systemDark.value = e.matches))

  watch(
    [preference, systemDark],
    () => {
      document.documentElement.dataset.theme = resolveTheme(preference.value, systemDark.value)
    },
    { immediate: true },
  )

  watch(
    preference,
    (value) => {
      try {
        localStorage.setItem(THEME_KEY, value)
      } catch {
        // Storage unavailable: the choice lasts until the app closes.
      }
      // The title bar follows too; null hands it back to Windows. Outside Tauri this fails.
      try {
        getCurrentWindow()
          .setTheme(value === 'system' ? null : value)
          .catch(() => {})
      } catch {
        // Not running in a Tauri window (component tests, the browser dev page).
      }
    },
    { immediate: true },
  )

  return { preference }
})

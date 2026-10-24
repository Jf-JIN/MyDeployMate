import { defineStore } from 'pinia'
import { ref } from 'vue'

export const WELCOME_KEY = 'mydeploymate.welcomeSeen'

function readSeen(): boolean {
  try {
    return localStorage.getItem(WELCOME_KEY) === '1'
  } catch {
    return false
  }
}

/**
 * D06 welcome wizard (IXD-40). Whether it was seen is a per-machine fact like the theme: not
 * part of the workspace and not exported.
 */
export const useWelcomeStore = defineStore('welcome', () => {
  const seen = ref(readSeen())
  const open = ref(false)

  /** On startup: only the first time, and never over an active run. */
  function showOnStartup(runActive: boolean) {
    if (!seen.value && !runActive) open.value = true
  }

  function reopen() {
    open.value = true
  }

  function close() {
    open.value = false
    seen.value = true
    try {
      localStorage.setItem(WELCOME_KEY, '1')
    } catch {
      // Private storage unavailable: the wizard shows again next time, nothing else breaks.
    }
  }

  return { seen, open, showOnStartup, reopen, close }
})

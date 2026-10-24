import { Channel } from '@tauri-apps/api/core'
import { defineStore } from 'pinia'
import { ref } from 'vue'

import { call, toAppError, type Message, type UpdateInfo, type UpdateProgress } from '@/ipc'

export interface UpdateCheck {
  atMs: number
  /** The newer version, if the check found one. */
  version: string | null
  error: Message | null
}

/**
 * App updates (NFR-04, IXD-39, UI-34). A check never shows a dialog by itself: a new version
 * lights the rail icon, and a failure is only shown in the settings page.
 */
export const useUpdateStore = defineStore('update', () => {
  const available = ref<UpdateInfo | null>(null)
  const checking = ref(false)
  const lastCheck = ref<UpdateCheck | null>(null)
  const dialogOpen = ref(false)
  /** `installing`: downloaded and verified, the installer is starting and the app will exit. */
  const phase = ref<'idle' | 'downloading' | 'installing'>('idle')
  const progress = ref<UpdateProgress | null>(null)
  /** Why the last install attempt stopped, shown in the dialog. */
  const error = ref<Message | null>(null)

  async function check() {
    if (checking.value || phase.value !== 'idle') return
    checking.value = true
    try {
      const found = await call('check_update')
      available.value = found
      lastCheck.value = { atMs: Date.now(), version: found?.version ?? null, error: null }
    } catch (e) {
      lastCheck.value = { atMs: Date.now(), version: null, error: toAppError(e).message }
    } finally {
      checking.value = false
    }
  }

  /** On start, if the setting allows it (`Preferences.checkUpdates`). */
  function checkOnStartup(enabled: boolean) {
    if (enabled) void check()
  }

  function open() {
    error.value = null
    dialogOpen.value = true
  }

  /** Closing while downloading stops the download; nothing has been installed then. */
  function close() {
    if (phase.value === 'installing') return
    if (phase.value === 'downloading') void cancel()
    dialogOpen.value = false
  }

  async function install() {
    if (phase.value !== 'idle' || !available.value) return
    error.value = null
    progress.value = null
    phase.value = 'downloading'
    const channel = new Channel<UpdateProgress>()
    channel.onmessage = (p) => {
      progress.value = p
      if (p.total !== null && p.downloaded >= p.total) phase.value = 'installing'
    }
    try {
      // On success the installer takes over and the app exits, so this does not return.
      await call('install_update', { onProgress: channel })
      phase.value = 'idle'
    } catch (e) {
      error.value = toAppError(e).message
      phase.value = 'idle'
    }
  }

  async function cancel() {
    await call('cancel_update').catch(() => {})
  }

  return {
    available,
    checking,
    lastCheck,
    dialogOpen,
    phase,
    progress,
    error,
    check,
    checkOnStartup,
    open,
    close,
    install,
    cancel,
  }
})

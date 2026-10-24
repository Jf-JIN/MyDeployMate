import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { UpdateInfo, UpdateProgress } from '@/ipc'

const call = vi.fn()
vi.mock('@/ipc', async (original) => ({
  ...(await original<typeof import('@/ipc')>()),
  call: (...args: unknown[]) => call(...args),
}))

let channel: { onmessage: (p: UpdateProgress) => void } | null = null
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {
    onmessage: (p: UpdateProgress) => void = () => {}
    constructor() {
      // eslint-disable-next-line @typescript-eslint/no-this-alias
      channel = this
    }
  },
}))

const { useUpdateStore } = await import('./update')

const info: UpdateInfo = { version: '0.2.0', currentVersion: '0.1.0', notes: 'Fixes' }
const failure = {
  kind: 'update',
  message: { code: 'update.checkFailed', params: { detail: 'offline' } },
  validation: null,
}

describe('update store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    call.mockReset()
    channel = null
  })

  it('checks on start only when the setting allows it', async () => {
    const update = useUpdateStore()
    update.checkOnStartup(false)
    expect(call).not.toHaveBeenCalled()
    call.mockResolvedValueOnce(info)
    update.checkOnStartup(true)
    await vi.waitFor(() => expect(update.available).toEqual(info))
    expect(update.lastCheck?.version).toBe('0.2.0')
    // Finding a version never opens the dialog by itself (IXD-39).
    expect(update.dialogOpen).toBe(false)
  })

  it('keeps a failed check quiet and records it for the settings page', async () => {
    const update = useUpdateStore()
    call.mockRejectedValueOnce(failure)
    await update.check()
    expect(update.available).toBeNull()
    expect(update.lastCheck?.error).toEqual(failure.message)
    expect(update.dialogOpen).toBe(false)
  })

  it('reports download progress, then the installer phase', async () => {
    const update = useUpdateStore()
    call.mockResolvedValueOnce(info)
    await update.check()
    let finish: (v: string) => void = () => {}
    call.mockReturnValueOnce(new Promise((resolve) => (finish = resolve)))
    const installing = update.install()
    expect(update.phase).toBe('downloading')
    expect(call).toHaveBeenLastCalledWith('install_update', { onProgress: channel })
    channel!.onmessage({ downloaded: 10, total: 20 })
    expect(update.progress).toEqual({ downloaded: 10, total: 20 })
    channel!.onmessage({ downloaded: 20, total: 20 })
    expect(update.phase).toBe('installing')
    finish('cancelled')
    await installing
    expect(update.phase).toBe('idle')
  })

  it('cancels the download when the dialog closes, and shows install errors', async () => {
    const update = useUpdateStore()
    call.mockResolvedValueOnce(info)
    await update.check()
    update.open()
    let finish: (v: string) => void = () => {}
    call.mockReturnValueOnce(new Promise((resolve) => (finish = resolve)))
    const installing = update.install()
    call.mockResolvedValueOnce(null)
    update.close()
    expect(call).toHaveBeenLastCalledWith('cancel_update')
    expect(update.dialogOpen).toBe(false)
    finish('cancelled')
    await installing

    const invalid = { ...failure, message: { code: 'update.signatureInvalid' } }
    call.mockRejectedValueOnce(invalid)
    await update.install()
    expect(update.error).toEqual(invalid.message)
    expect(update.phase).toBe('idle')
  })
})

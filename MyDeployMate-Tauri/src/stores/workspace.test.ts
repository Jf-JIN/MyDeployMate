import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { Workspace } from '@/ipc'

const call = vi.fn()
vi.mock('@/ipc', async (original) => ({
  ...(await original<typeof import('@/ipc')>()),
  call: (...args: unknown[]) => call(...args),
}))

const { useWorkspaceStore } = await import('./workspace')

const workspace: Workspace = {
  schemaVersion: 1,
  revision: 3,
  environments: [{ id: 'env-1', name: 'staging', protected: false }],
  servers: [],
  projects: [],
  globalVariables: [],
  preferences: { checkUpdates: true },
}

describe('workspace store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    call.mockReset()
  })

  it('loads the workspace and exposes environment names', async () => {
    call.mockResolvedValue({ workspace, notice: { kind: 'created' } })
    const store = useWorkspaceStore()
    await store.load()
    expect(store.error).toBeNull()
    expect(store.notice).toEqual({ kind: 'created' })
    expect(store.environmentName('env-1')).toBe('staging')
    expect(store.environmentName('missing')).toBe('（已删除的环境）')
  })

  it('keeps the error when loading fails', async () => {
    call.mockRejectedValue({
      kind: 'locked',
      message: { code: 'store.locked', params: { path: 'workspace.json' } },
      validation: null,
    })
    const store = useWorkspaceStore()
    await store.load()
    expect(store.workspace).toBeNull()
    expect(store.error?.kind).toBe('locked')
    expect(store.loading).toBe(false)
  })

  it('replaces the workspace with the saved copy', async () => {
    call.mockResolvedValue({ ...workspace, revision: 4 })
    const store = useWorkspaceStore()
    const saved = await store.save(workspace)
    expect(call).toHaveBeenCalledWith('save_workspace', { draft: workspace })
    expect(saved.revision).toBe(4)
    expect(store.workspace?.revision).toBe(4)
  })
})

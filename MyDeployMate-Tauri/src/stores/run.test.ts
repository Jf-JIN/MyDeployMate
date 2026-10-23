import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { Run, RunSnapshot, RunUpdate } from '@/ipc'

const call = vi.fn()
vi.mock('@/ipc', async (original) => ({
  ...(await original<typeof import('@/ipc')>()),
  call: (...args: unknown[]) => call(...args),
}))

// A stand-in for Tauri's channel that the test can push updates through.
let channel: { onmessage: (u: RunUpdate) => void } | null = null
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {
    onmessage: (u: RunUpdate) => void = () => {}
    constructor() {
      // eslint-disable-next-line @typescript-eslint/no-this-alias
      channel = this
    }
  },
}))

const { useRunStore } = await import('./run')

const run: Run = {
  id: 'run-1',
  projectId: 'p',
  pipelineId: 'f',
  environmentId: 'e',
  projectName: '官网',
  pipelineName: '发布',
  environmentName: 'staging',
  version: '1.0',
  executionOrder: 'stepFirst',
  localOnly: true,
  status: 'running',
  createdAtMs: 0,
  finishedAtMs: null,
}

function snapshot(over: Partial<RunSnapshot> = {}): RunSnapshot {
  return {
    run,
    tasks: [],
    cancelReason: null,
    plan: { tasks: [], targets: [] } as unknown as RunSnapshot['plan'],
    logs: [{ n: 0, task: 0, source: 'stdout', text: 'a\n', atMs: 0 }],
    damagedLines: 0,
    active: true,
    request: null,
    ...over,
  }
}

describe('run store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    call.mockReset()
    channel = null
  })

  it('starts a run, opens it and applies updates sent while it was opening', async () => {
    call.mockImplementation(async (cmd: string) => {
      if (cmd === 'start_run') return 'run-1'
      // An update that overtakes the snapshot: newer than it, applied after it.
      channel?.onmessage({
        type: 'logs',
        entries: [
          { n: 0, task: 0, source: 'stdout', text: 'a\n', atMs: 0 },
          { n: 1, task: 0, source: 'stdout', text: 'b\n', atMs: 0 },
        ],
      })
      return snapshot()
    })
    const store = useRunStore()
    const id = await store.start({
      request: {} as never,
      revision: 1,
      confirmation: null,
      key: 'k',
    })
    expect(id).toBe('run-1')
    expect(call).toHaveBeenCalledWith('open_run', expect.objectContaining({ runId: 'run-1' }))
    const d = store.detail('run-1')
    expect(d?.lines.lines.map((l) => l.text)).toEqual(['a', 'b'])
    expect(store.active?.projectName).toBe('官网')

    channel?.onmessage({
      type: 'state',
      run: { ...run, status: 'waitingConfirmation' },
      tasks: [],
      cancelReason: null,
    })
    expect(store.active?.waiting).toBe('confirm')

    channel?.onmessage({
      type: 'state',
      run: { ...run, status: 'succeeded', finishedAtMs: 5 },
      tasks: [],
      cancelReason: null,
    })
    expect(store.active).toBeNull()
    expect(store.activeId).toBeNull()
    expect(store.detail('run-1')?.live).toBe(false)
  })

  it('opens an ended run without making it active', async () => {
    call.mockResolvedValue(snapshot({ run: { ...run, status: 'failed' }, active: false }))
    const store = useRunStore()
    const d = await store.open('run-1')
    expect(d.live).toBe(false)
    expect(store.activeId).toBeNull()
    expect(await store.open('run-1')).toBe(d)
    expect(call).toHaveBeenCalledTimes(1)
  })

  it('keeps the open trust request until the run stops waiting, and answers it', async () => {
    call.mockResolvedValue(snapshot())
    const store = useRunStore()
    await store.open('run-1')
    const waiting = { ...run, status: 'waitingCredential' as const }
    channel?.onmessage({ type: 'state', run: waiting, tasks: [], cancelReason: null })
    channel?.onmessage({
      type: 'request',
      requestId: 4,
      task: 1,
      prompt: {
        type: 'hostTrust',
        serverId: 's1',
        serverName: 'web-1',
        user: 'deploy',
        endpoint: { host: '10.0.0.5', port: 22 },
        presented: { algorithm: 'ssh-ed25519', fingerprint: 'SHA256:abc' },
      },
    })
    expect(store.detail('run-1')?.request?.requestId).toBe(4)
    expect(store.active?.waiting).toBe('trust')

    await store.answerTrust('run-1', 4, true)
    expect(call).toHaveBeenCalledWith('answer_run_trust', {
      runId: 'run-1',
      requestId: 4,
      trust: true,
    })
    channel?.onmessage({ type: 'state', run, tasks: [], cancelReason: null })
    expect(store.detail('run-1')?.request).toBeNull()
  })
})

import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { ConnectionTest, TestEvent } from '@/ipc'

const call = vi.fn()
vi.mock('@/ipc', async (original) => ({
  ...(await original<typeof import('@/ipc')>()),
  call: (...args: unknown[]) => call(...args),
}))

let channel: { onmessage: (e: TestEvent) => void } | null = null
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {
    onmessage: (e: TestEvent) => void = () => {}
    constructor() {
      // eslint-disable-next-line @typescript-eslint/no-this-alias
      channel = this
    }
  },
}))

const { useConnectionTestStore } = await import('./connectionTests')

const ok = (serverId: string): ConnectionTest => ({
  serverId,
  startedAtMs: 1,
  durationMs: 2,
  outcome: { status: 'succeeded', system: 'Linux 6.8 x86_64' },
})

describe('connection test store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    call.mockReset()
    channel = null
  })

  it('queues first-trust requests and answers the head of the queue', async () => {
    const store = useConnectionTestStore()
    let finish: (t: ConnectionTest) => void = () => {}
    call.mockImplementation((cmd: string) =>
      cmd === 'test_connection' ? new Promise((r) => (finish = r)) : Promise.resolve(null),
    )
    const pending = store.test('s1', { name: 'web-1', user: 'deploy' })
    expect(store.running.s1).toBeDefined()
    channel!.onmessage({
      requestId: 7,
      prompt: {
        type: 'hostTrust',
        serverId: 's1',
        serverName: 'web-1',
        user: 'deploy',
        endpoint: { host: '10.0.0.5', port: 22 },
        presented: { algorithm: 'ssh-ed25519', fingerprint: 'SHA256:abc' },
      },
    })
    expect(store.pendingTrust?.requestId).toBe(7)
    await store.answerTrust(true)
    expect(call).toHaveBeenCalledWith('answer_host_trust', { requestId: 7, trust: true })
    expect(store.pendingTrust).toBeNull()
    finish(ok('s1'))
    await pending
    expect(store.running.s1).toBeUndefined()
    expect(store.results.s1?.outcome.status).toBe('succeeded')
  })

  it('opens the key-change notice when a test is blocked', async () => {
    const store = useConnectionTestStore()
    call.mockResolvedValue({
      serverId: 's1',
      startedAtMs: 1,
      durationMs: 2,
      outcome: {
        status: 'failed',
        error: {
          stage: 'hostKey',
          kind: 'hostKeyChanged',
          endpoint: { host: 'h', port: 22 },
          known: [{ algorithm: 'ssh-ed25519', fingerprint: 'SHA256:old' }],
          presented: { algorithm: 'ssh-ed25519', fingerprint: 'SHA256:new' },
          message: '服务器身份与已记录的不一致，连接已停止',
        },
      },
    })
    await store.test('s1', { name: 'web-1', user: 'deploy' })
    expect(store.keyChange?.presented.fingerprint).toBe('SHA256:new')
    expect(store.keyChange?.known[0]?.fingerprint).toBe('SHA256:old')
  })

  it('runs a batch one by one and stops after cancel', async () => {
    const store = useConnectionTestStore()
    const order: string[] = []
    call.mockImplementation(async (cmd: string, args: { serverId?: string }) => {
      if (cmd === 'test_connection') {
        order.push(args.serverId!)
        if (args.serverId === 'a') await store.cancelBatch()
        return ok(args.serverId!)
      }
      return null
    })
    await store.testAll([
      { id: 'a', name: 'a', user: 'u' },
      { id: 'b', name: 'b', user: 'u' },
    ])
    expect(order).toEqual(['a'])
    expect(call).toHaveBeenCalledWith('cancel_connection_test', expect.anything())
    expect(store.batch).toBeNull()
  })

  it('queues D04 requests per server and drops them when the test ends', async () => {
    const store = useConnectionTestStore()
    let finish: (t: ConnectionTest) => void = () => {}
    call.mockImplementation((cmd: string) =>
      cmd === 'test_connection' ? new Promise((r) => (finish = r)) : Promise.resolve(null),
    )
    const pending = store.test('s1', { name: 'web-1', user: 'deploy' })
    channel!.onmessage({
      requestId: 3,
      prompt: {
        type: 'credential',
        purpose: 'sshPassword',
        serverName: 'web-1',
        address: '10.0.0.5:22',
        user: 'deploy',
        keyPath: null,
        context: '连接测试',
        attempt: 1,
        maxAttempts: 3,
        rejected: false,
        savedRejected: false,
        rememberDefault: false,
      },
    })
    expect(store.pendingCredential?.requestId).toBe(3)
    await store.answerCredential('pw', true)
    expect(call).toHaveBeenCalledWith('submit_secret', {
      requestId: 3,
      secret: 'pw',
      remember: true,
    })
    expect(store.pendingCredential).toBeNull()
    channel!.onmessage({
      requestId: 4,
      prompt: {
        type: 'credential',
        purpose: 'sshPassword',
        serverName: 'web-1',
        address: '10.0.0.5:22',
        user: 'deploy',
        keyPath: null,
        context: '连接测试',
        attempt: 2,
        maxAttempts: 3,
        rejected: true,
        savedRejected: false,
        rememberDefault: false,
      },
    })
    finish(ok('s1'))
    await pending
    expect(store.pendingCredential).toBeNull()
  })
})

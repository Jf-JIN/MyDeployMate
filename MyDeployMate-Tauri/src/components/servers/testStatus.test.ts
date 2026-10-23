import { describe, expect, it } from 'vitest'

import type { ConnectError, ConnectionTest, Message } from '@/ipc'

import { fixAction, testStatus } from './testStatus'

const none: Message = { code: 'text', params: { text: '' } }

function failed(error: ConnectError): ConnectionTest {
  return { serverId: 's', startedAtMs: 0, durationMs: 10, outcome: { status: 'failed', error } }
}

const legacy: ConnectError = {
  stage: 'handshake',
  kind: 'algorithmNegotiation',
  legacyRsaOnly: true,
  message: { code: 'connect.legacyRsaOnly' },
}

describe('testStatus', () => {
  it('distinguishes untested, running, success and specific failures', () => {
    expect(testStatus(undefined, false).label).toBe('未测试')
    expect(testStatus(undefined, true)).toMatchObject({ label: '测试中', spin: true })
    expect(
      testStatus(
        {
          serverId: 's',
          startedAtMs: 0,
          durationMs: 1,
          outcome: { status: 'succeeded', system: 'Linux' },
        },
        false,
      ),
    ).toMatchObject({ label: '连接正常', tone: 'success' })
    expect(testStatus(failed(legacy), false).label).toBe('算法不兼容')
    expect(
      testStatus(
        failed({ stage: 'authenticate', kind: 'authenticationFailed', message: none }),
        false,
      ).label,
    ).toBe('认证失败')
    expect(
      testStatus(failed({ stage: 'authenticate', kind: 'agentNoIdentities', message: none }), false)
        .label,
    ).toBe('Agent 无密钥')
  })

  it('never calls a failure "offline"', () => {
    const kinds: ConnectError[] = [
      { stage: 'resolve', kind: 'resolveFailed', message: none },
      { stage: 'connect', kind: 'connectionRefused', message: none },
      { stage: 'connect', kind: 'timeout', message: none },
    ]
    for (const k of kinds) expect(testStatus(failed(k), false).label).not.toContain('离线')
  })
})

describe('fixAction', () => {
  it('sends legacy-only and auth failures to the server form, key changes to trust management', () => {
    expect(fixAction(legacy)).toBe('editServer')
    expect(
      fixAction({
        stage: 'hostKey',
        kind: 'hostKeyChanged',
        endpoint: { host: 'h', port: 22 },
        known: [],
        presented: { algorithm: 'ssh-ed25519', fingerprint: 'SHA256:x' },
        message: none,
      }),
    ).toBe('trustManagement')
    expect(fixAction({ stage: 'handshake', kind: 'disconnected', message: none })).toBeNull()
  })
})

import { describe, expect, it } from 'vitest'

import type { ConnectError, ConnectionTest } from '@/ipc'

import { fixAction, testStatus } from './testStatus'

function failed(error: ConnectError): ConnectionTest {
  return { serverId: 's', startedAtMs: 0, durationMs: 10, outcome: { status: 'failed', error } }
}

const legacy: ConnectError = {
  stage: 'handshake',
  kind: 'algorithmNegotiation',
  legacyRsaOnly: true,
  message: '该服务器只支持 ssh-rsa，可在服务器设置中开启“允许旧算法”',
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
        failed({ stage: 'authenticate', kind: 'authenticationFailed', message: '' }),
        false,
      ).label,
    ).toBe('认证失败')
  })

  it('never calls a failure "offline"', () => {
    const kinds: ConnectError[] = [
      { stage: 'resolve', kind: 'resolveFailed', message: '' },
      { stage: 'connect', kind: 'connectionRefused', message: '' },
      { stage: 'connect', kind: 'timeout', message: '' },
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
        message: '',
      }),
    ).toBe('trustManagement')
    expect(fixAction({ stage: 'handshake', kind: 'disconnected', message: '' })).toBeNull()
  })
})

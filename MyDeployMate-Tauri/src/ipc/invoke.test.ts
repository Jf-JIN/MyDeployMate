import { beforeEach, describe, expect, it, vi } from 'vitest'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

const { call, toAppError } = await import('./invoke')

async function rejection(promise: Promise<unknown>): Promise<unknown> {
  try {
    await promise
  } catch (error) {
    return error
  }
  throw new Error('expected the promise to reject')
}

describe('call', () => {
  beforeEach(() => {
    invoke.mockReset()
  })

  it('passes the command name and arguments through', async () => {
    invoke.mockResolvedValue({
      version: '0.1.0',
      dataDir: 'C:\\Users\\me\\AppData\\Roaming\\MyDeployMate',
    })
    const info = await call('app_info')
    expect(invoke).toHaveBeenCalledWith('app_info', undefined)
    expect(info.version).toBe('0.1.0')
  })

  it('rethrows backend errors unchanged', async () => {
    const appError = {
      kind: 'revisionConflict',
      message: { code: 'store.externalChange' },
      validation: null,
    }
    invoke.mockRejectedValue(appError)
    expect(await rejection(call('load_workspace'))).toEqual(appError)
  })

  it('wraps unexpected rejections as internal errors', async () => {
    invoke.mockRejectedValue('command not found')
    expect(await rejection(call('load_workspace'))).toEqual({
      kind: 'internal',
      message: { code: 'text', params: { text: 'command not found' } },
      validation: null,
    })
  })
})

describe('toAppError', () => {
  it('keeps the message of an Error', () => {
    expect(toAppError(new Error('boom')).message).toEqual({
      code: 'text',
      params: { text: 'boom' },
    })
  })
})

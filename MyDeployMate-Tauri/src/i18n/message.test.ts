import { afterEach, describe, expect, it } from 'vitest'

import labelTs from '@/ipc/bindings/Label.ts?raw'
import messageTs from '@/ipc/bindings/Message.ts?raw'

import { i18n } from './index'
import { errorText, messageText } from './message'
import zhCN from './zh-CN'

/** Each variant of a generated union as its code and parameter names. */
function variants(source: string): [string, string[]][] {
  return [...source.matchAll(/\{ "code": "([^"]+)"(?:, "params": \{ ([^}]*) \})? \}/g)].map((m) => [
    m[1]!,
    [...(m[2] ?? '').matchAll(/(\w+):/g)].map((p) => p[1]!).sort(),
  ])
}

const backend = zhCN.backend as Record<string, unknown>

function text(code: string): string {
  let node: unknown = backend
  for (const part of code.split('.')) node = (node as Record<string, unknown>)?.[part]
  return node as string
}

describe('backend messages', () => {
  afterEach(() => {
    i18n.global.locale.value = 'zh-CN'
  })

  // AT-18: every code Rust can send has Chinese text that uses exactly its parameters.
  it('has Chinese text for every code with matching placeholders', () => {
    const all = [...variants(messageTs), ...variants(labelTs)]
    expect(all.length).toBeGreaterThan(200)
    for (const [code, params] of all) {
      const source = text(code)
      expect(typeof source, code).toBe('string')
      const names = [...new Set([...source.matchAll(/\{(\w+)\}/g)].map((m) => m[1]!))].sort()
      expect(names, code).toEqual(params)
    }
  })

  it('renders nested messages and labels', () => {
    expect(
      messageText({ code: 'validate.required', params: { label: { code: 'label.port' } } }),
    ).toBe(`${text('label.port')}不能为空`)
    expect(
      messageText({
        code: 'run.connectFailed',
        params: { server: 'web-1', reason: { code: 'connect.timeout' } },
      }),
    ).toBe(`连接 web-1 失败：${text('connect.timeout')}`)
  })

  it('shows text saved before message codes as it was', () => {
    expect(messageText({ code: 'text', params: { text: '退出码 1' } })).toBe('退出码 1')
    expect(errorText(new Error('boom'))).toBe('boom')
  })

  it('follows the current language', () => {
    i18n.global.locale.value = 'en-US'
    // Untranslated until 8.4c: falls back to Chinese rather than showing the key.
    expect(messageText({ code: 'run.userCancelled' })).toBe(text('run.userCancelled'))
  })
})

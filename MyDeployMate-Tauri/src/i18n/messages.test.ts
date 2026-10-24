import { afterEach, describe, expect, it } from 'vitest'

import enUS from './en-US'
import { i18n, translate, type MessageKey } from './index'
import zhCN from './zh-CN'

function leaves(node: object, prefix = ''): [string, string][] {
  return Object.entries(node).flatMap(([key, value]) =>
    typeof value === 'string'
      ? [[prefix + key, value] as [string, string]]
      : leaves(value as object, `${prefix}${key}.`),
  )
}

describe('messages', () => {
  afterEach(() => {
    i18n.global.locale.value = 'zh-CN'
  })

  it.each([
    ['zh-CN', zhCN],
    ['en-US', enUS],
  ] as const)('compiles every %s message and fills its placeholders', (locale, messages) => {
    i18n.global.locale.value = locale
    for (const [key, source] of leaves(messages)) {
      const names = [...source.matchAll(/\{(\w+)\}/g)].map((m) => m[1]!)
      const params = Object.fromEntries(names.map((n) => [n, `[[${n}]]`]))
      const text = translate(key as MessageKey, params)
      expect(text, key).not.toBe(key)
      for (const n of names) expect(text, key).toContain(`[[${n}]]`)
      // `{'}'}` is a literal brace, not a placeholder.
      if (!source.includes("{'")) expect(text.replace(/\[\[\w+\]\]/g, ''), key).not.toMatch(/[{}]/)
    }
  })

  // AC-33: English has every key and the same placeholders; the type checks the keys too.
  it('uses the same keys and placeholders in English as in Chinese', () => {
    const en = new Map(leaves(enUS))
    const zh = leaves(zhCN)
    expect([...en.keys()].sort()).toEqual(zh.map(([key]) => key).sort())
    const names = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort()
    for (const [key, source] of zh) expect(names(en.get(key)!), key).toEqual(names(source))
    expect(i18n.global.availableLocales).toContain('en-US')
  })

  // The interface shows no Chinese in English, apart from language names (IXD-36).
  it('has no Chinese in English messages', () => {
    for (const [key, text] of leaves(enUS))
      expect(text, key).not.toMatch(/[\u3000-\u9fff\uff00-\uffef]/)
  })
})

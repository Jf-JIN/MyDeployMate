import { describe, expect, it } from 'vitest'

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
  it('compiles every Chinese message and fills its placeholders', () => {
    for (const [key, source] of leaves(zhCN)) {
      const names = [...source.matchAll(/\{(\w+)\}/g)].map((m) => m[1]!)
      const params = Object.fromEntries(names.map((n) => [n, `[[${n}]]`]))
      const text = translate(key as MessageKey, params)
      expect(text, key).not.toBe(key)
      for (const n of names) expect(text, key).toContain(`[[${n}]]`)
      expect(text.replace(/\[\[\w+\]\]/g, ''), key).not.toMatch(/[{}]/)
    }
  })

  it('uses the same placeholders in English as in Chinese', () => {
    const zh = new Map(leaves(zhCN))
    for (const [key, en] of leaves(enUS)) {
      const names = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort()
      expect(names(en), key).toEqual(names(zh.get(key)!))
    }
    expect(i18n.global.availableLocales).toContain('en-US')
  })
})

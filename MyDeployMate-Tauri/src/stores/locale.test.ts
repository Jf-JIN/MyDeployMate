import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'
import { nextTick } from 'vue'

import { i18n, translate } from '@/i18n'

import { LOCALE_KEY, parseLocalePreference, resolveLocale, useLocaleStore } from './locale'

describe('locale', () => {
  beforeEach(() => {
    localStorage.clear()
    setActivePinia(createPinia())
  })

  it('follows the Windows display language unless a language is chosen', () => {
    expect(resolveLocale('system', 'zh-CN')).toBe('zh-CN')
    expect(resolveLocale('system', 'zh-TW')).toBe('zh-CN')
    expect(resolveLocale('system', 'en-GB')).toBe('en-US')
    expect(resolveLocale('system', 'de-DE')).toBe('en-US')
    expect(resolveLocale('en-US', 'zh-CN')).toBe('en-US')
    expect(resolveLocale('zh-CN', 'en-US')).toBe('zh-CN')
  })

  it('treats a missing or unknown stored value as follow system', () => {
    expect(parseLocalePreference(null)).toBe('system')
    expect(parseLocalePreference('fr-FR')).toBe('system')
    expect(parseLocalePreference('en-US')).toBe('en-US')
  })

  it('switches messages and <html lang> at once and keeps the choice', async () => {
    const store = useLocaleStore()
    store.preference = 'en-US'
    await nextTick()
    expect(i18n.global.locale.value).toBe('en-US')
    expect(document.documentElement.lang).toBe('en-US')
    expect(translate('settings.look.title')).toBe('Appearance')
    expect(localStorage.getItem(LOCALE_KEY)).toBe('en-US')

    store.preference = 'zh-CN'
    await nextTick()
    expect(translate('settings.look.title')).toBe('外观')
    expect(document.documentElement.lang).toBe('zh-CN')
  })

  it('falls back to Chinese for an English entry that is not translated yet', () => {
    i18n.global.mergeLocaleMessage('zh-CN', { probe: '仅中文' })
    expect(i18n.global.t('probe' as never, {}, { locale: 'en-US' })).toBe('仅中文')
  })
})

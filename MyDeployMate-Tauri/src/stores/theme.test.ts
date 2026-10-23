import { describe, expect, it } from 'vitest'

import { parsePreference, resolveTheme } from './theme'

describe('theme', () => {
  it('follows the system unless a theme is chosen', () => {
    expect(resolveTheme('system', true)).toBe('dark')
    expect(resolveTheme('system', false)).toBe('light')
    expect(resolveTheme('light', true)).toBe('light')
    expect(resolveTheme('dark', false)).toBe('dark')
  })

  it('treats a missing or unknown stored value as follow system', () => {
    expect(parsePreference(null)).toBe('system')
    expect(parsePreference('blue')).toBe('system')
    expect(parsePreference('dark')).toBe('dark')
  })
})

import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'

import { WELCOME_KEY, useWelcomeStore } from './welcome'

describe('welcome wizard', () => {
  beforeEach(() => {
    localStorage.clear()
    setActivePinia(createPinia())
  })

  it('opens on the first start only, and not over an active run', () => {
    const welcome = useWelcomeStore()
    welcome.showOnStartup(true)
    expect(welcome.open).toBe(false)
    welcome.showOnStartup(false)
    expect(welcome.open).toBe(true)
    welcome.close()
    expect(localStorage.getItem(WELCOME_KEY)).toBe('1')

    setActivePinia(createPinia())
    const later = useWelcomeStore()
    later.showOnStartup(false)
    expect(later.open).toBe(false)
    later.reopen()
    expect(later.open).toBe(true)
  })
})

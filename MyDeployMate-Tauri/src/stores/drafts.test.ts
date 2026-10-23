import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick, ref } from 'vue'

import type { MessageKey } from '@/i18n'

import { useDraftStore } from './drafts'

function source(label: MessageKey, saveResult = true) {
  const dirty = ref(false)
  return {
    dirty,
    source: {
      label,
      isDirty: () => dirty.value,
      save: vi.fn(async () => {
        if (saveResult) dirty.value = false
        return saveResult
      }),
      discard: vi.fn(() => (dirty.value = false)),
    },
  }
}

describe('draft store', () => {
  beforeEach(() => setActivePinia(createPinia()))

  it('lets navigation through when nothing is dirty', async () => {
    const store = useDraftStore()
    store.register(source('drafts.labels.pipeline').source)
    await expect(store.confirmLeave()).resolves.toBe(true)
    expect(store.pending).toBeNull()
  })

  it('tracks dirtiness reactively and unregisters', () => {
    const store = useDraftStore()
    const a = source('drafts.labels.pipeline')
    const unregister = store.register(a.source)
    expect(store.anyDirty).toBe(false)
    a.dirty.value = true
    expect(store.anyDirty).toBe(true)
    unregister()
    expect(store.anyDirty).toBe(false)
  })

  it.each([
    ['cancel', false, 0, 0],
    ['discard', true, 0, 1],
    ['save', true, 1, 0],
  ] as const)('answers %s', async (choice, leaves, saves, discards) => {
    const store = useDraftStore()
    const a = source('drafts.labels.pipeline')
    store.register(a.source)
    a.dirty.value = true
    const leaving = store.confirmLeave()
    await nextTick()
    expect(store.pending?.labels).toEqual(['drafts.labels.pipeline'])
    store.answer(choice)
    await expect(leaving).resolves.toBe(leaves)
    expect(a.source.save).toHaveBeenCalledTimes(saves)
    expect(a.source.discard).toHaveBeenCalledTimes(discards)
    expect(store.pending).toBeNull()
  })

  it('stays when saving fails validation', async () => {
    const store = useDraftStore()
    const a = source('drafts.labels.targets', false)
    store.register(a.source)
    a.dirty.value = true
    const leaving = store.confirmLeave()
    await nextTick()
    store.answer('save')
    await expect(leaving).resolves.toBe(false)
    expect(store.anyDirty).toBe(true)
  })

  it('saves only the most recent dirty editor on Ctrl+S', async () => {
    const store = useDraftStore()
    const a = source('drafts.labels.pipeline')
    const b = source('drafts.labels.settings')
    store.register(a.source)
    store.register(b.source)
    a.dirty.value = true
    b.dirty.value = true
    await expect(store.saveCurrent()).resolves.toBe(true)
    expect(b.source.save).toHaveBeenCalledOnce()
    expect(a.source.save).not.toHaveBeenCalled()
  })
})

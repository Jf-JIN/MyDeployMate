import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick, ref } from 'vue'

import { useDraftStore } from './drafts'

function source(label: string, saveResult = true) {
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
    store.register(source('流程').source)
    await expect(store.confirmLeave()).resolves.toBe(true)
    expect(store.pending).toBeNull()
  })

  it('tracks dirtiness reactively and unregisters', () => {
    const store = useDraftStore()
    const a = source('流程 发布流程')
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
    const a = source('流程 发布流程')
    store.register(a.source)
    a.dirty.value = true
    const leaving = store.confirmLeave()
    await nextTick()
    expect(store.pending?.labels).toEqual(['流程 发布流程'])
    store.answer(choice)
    await expect(leaving).resolves.toBe(leaves)
    expect(a.source.save).toHaveBeenCalledTimes(saves)
    expect(a.source.discard).toHaveBeenCalledTimes(discards)
    expect(store.pending).toBeNull()
  })

  it('stays when saving fails validation', async () => {
    const store = useDraftStore()
    const a = source('服务器 api-01', false)
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
    const a = source('流程')
    const b = source('服务器')
    store.register(a.source)
    store.register(b.source)
    a.dirty.value = true
    b.dirty.value = true
    await expect(store.saveCurrent()).resolves.toBe(true)
    expect(b.source.save).toHaveBeenCalledOnce()
    expect(a.source.save).not.toHaveBeenCalled()
  })
})

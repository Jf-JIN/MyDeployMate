import { defineStore } from 'pinia'
import { computed, ref, shallowRef } from 'vue'

import type { Workspace } from '@/ipc'

/** An editor that holds unsaved changes, such as the pipeline editor or the server form. */
export interface DraftSource {
  /** What the guard dialog names, for example "流程 发布流程". */
  label: string
  /** Read reactive state here so the guard updates as the draft changes. */
  isDirty: () => boolean
  /** Resolve true when saved; false when validation failed and the user must fix fields. */
  save: () => Promise<boolean>
  discard: () => void
  /** The edited workspace, for previews of unsaved changes (IXD-21). */
  snapshot?: () => Workspace | null
}

export type LeaveChoice = 'save' | 'discard' | 'cancel'

/**
 * Explicit-save drafts and the leave guard (IXD-03). Editors register while mounted; switching
 * project, pipeline, environment or page asks first when any of them is dirty.
 */
export const useDraftStore = defineStore('drafts', () => {
  const sources = shallowRef<DraftSource[]>([])
  const pending = ref<{ labels: string[]; resolve: (choice: LeaveChoice) => void } | null>(null)
  const saving = ref(false)

  const dirtySources = computed(() => sources.value.filter((s) => s.isDirty()))
  const anyDirty = computed(() => dirtySources.value.length > 0)

  function register(source: DraftSource): () => void {
    sources.value = [...sources.value, source]
    return () => {
      sources.value = sources.value.filter((s) => s !== source)
    }
  }

  /** Save the most recently registered dirty editor (Ctrl+S and the context bar's Save). */
  async function saveCurrent(): Promise<boolean> {
    const dirty = dirtySources.value
    const current = dirty[dirty.length - 1]
    if (!current) return true
    return runSave([current])
  }

  async function runSave(list: DraftSource[]): Promise<boolean> {
    saving.value = true
    try {
      for (const source of list) {
        if (!(await source.save())) return false
      }
      return true
    } finally {
      saving.value = false
    }
  }

  /** Resolve true when it is fine to leave; asks "保存并继续 / 放弃更改 / 取消" when dirty. */
  async function confirmLeave(): Promise<boolean> {
    const dirty = dirtySources.value
    if (dirty.length === 0) return true
    const choice = await new Promise<LeaveChoice>((resolve) => {
      pending.value = { labels: dirty.map((s) => s.label), resolve }
    })
    pending.value = null
    if (choice === 'cancel') return false
    if (choice === 'discard') {
      dirty.forEach((s) => s.discard())
      return true
    }
    return runSave(dirty)
  }

  function answer(choice: LeaveChoice) {
    pending.value?.resolve(choice)
  }

  return {
    anyDirty,
    dirtySources,
    pending,
    saving,
    register,
    saveCurrent,
    confirmLeave,
    answer,
  }
})

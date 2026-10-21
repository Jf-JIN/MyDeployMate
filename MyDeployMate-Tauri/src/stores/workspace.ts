import { defineStore } from 'pinia'
import { computed, ref, shallowRef } from 'vue'

import { call, toAppError, type AppError, type LoadedWorkspace, type Workspace } from '@/ipc'

/** The saved workspace as last loaded or saved by the backend. Drafts live in the editing views. */
export const useWorkspaceStore = defineStore('workspace', () => {
  const workspace = shallowRef<Workspace | null>(null)
  const notice = ref<LoadedWorkspace['notice']>(null)
  const loading = ref(false)
  const error = ref<AppError | null>(null)

  const environments = computed(() => workspace.value?.environments ?? [])
  const servers = computed(() => workspace.value?.servers ?? [])
  const projects = computed(() => workspace.value?.projects ?? [])

  function environmentName(id: string): string {
    return environments.value.find((e) => e.id === id)?.name ?? '（已删除的环境）'
  }

  async function load() {
    loading.value = true
    error.value = null
    try {
      const loaded = await call('load_workspace')
      workspace.value = loaded.workspace
      notice.value = loaded.notice
    } catch (e) {
      error.value = toAppError(e)
    } finally {
      loading.value = false
    }
  }

  /** Save a draft; on success the store holds the saved copy with its new revision. */
  async function save(draft: Workspace): Promise<Workspace> {
    const saved = await call('save_workspace', { draft })
    workspace.value = saved
    return saved
  }

  return {
    workspace,
    notice,
    loading,
    error,
    environments,
    servers,
    projects,
    environmentName,
    load,
    save,
  }
})

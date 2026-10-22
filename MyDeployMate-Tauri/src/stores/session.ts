import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

const KEY = 'mydeploymate.session'

interface Persisted {
  lastProjectId: string | null
  /** Last opened pipeline per project (IXD-02: reopen where the user left off). */
  lastPipeline: Record<string, string>
  /** Selected environment per project; falls back to the project's default environment. */
  environment: Record<string, string>
  treeCollapsed: boolean
}

function read(): Persisted {
  const empty: Persisted = {
    lastProjectId: null,
    lastPipeline: {},
    environment: {},
    treeCollapsed: false,
  }
  try {
    const raw = localStorage.getItem(KEY)
    return raw ? { ...empty, ...(JSON.parse(raw) as Partial<Persisted>) } : empty
  } catch {
    return empty
  }
}

/**
 * Per-machine UI state that is not configuration: where the user was and what is selected.
 * Lost storage only means starting from the project list again.
 */
export const useSessionStore = defineStore('session', () => {
  const initial = read()
  const lastProjectId = ref(initial.lastProjectId)
  const lastPipeline = ref(initial.lastPipeline)
  const environment = ref(initial.environment)
  const treeCollapsed = ref(initial.treeCollapsed)

  watch(
    [lastProjectId, lastPipeline, environment, treeCollapsed],
    () => {
      try {
        const state: Persisted = {
          lastProjectId: lastProjectId.value,
          lastPipeline: lastPipeline.value,
          environment: environment.value,
          treeCollapsed: treeCollapsed.value,
        }
        localStorage.setItem(KEY, JSON.stringify(state))
      } catch {
        // Storage unavailable: the session simply is not remembered.
      }
    },
    { deep: true },
  )

  function openPipeline(projectId: string, pipelineId: string) {
    lastProjectId.value = projectId
    lastPipeline.value = { ...lastPipeline.value, [projectId]: pipelineId }
  }

  function selectEnvironment(projectId: string, environmentId: string) {
    environment.value = { ...environment.value, [projectId]: environmentId }
  }

  return {
    lastProjectId,
    lastPipeline,
    environment,
    treeCollapsed,
    openPipeline,
    selectEnvironment,
  }
})

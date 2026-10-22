import { defineStore } from 'pinia'
import { ref } from 'vue'

/** What the shell shows about the single active run (SRS-1, IXD-02). */
export interface ActiveRunSummary {
  runId: string
  projectId: string
  projectName: string
  pipelineName: string
  environmentName: string
  /** Finished tasks over all tasks, for example 3/7 (UI-25). */
  done: number
  total: number
  waiting: 'confirm' | 'credential' | null
}

/**
 * The active run, if any. The execution engine fills this from phase 4; until then it stays
 * empty and the shell only reserves the entry point.
 */
export const useRunStore = defineStore('run', () => {
  const active = ref<ActiveRunSummary | null>(null)
  return { active }
})

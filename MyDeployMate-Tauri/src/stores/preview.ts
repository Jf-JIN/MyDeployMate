import { defineStore } from 'pinia'
import { ref, shallowRef } from 'vue'

import type { Issue } from '@/ipc'

/** A past run whose pipeline is about to be previewed again. */
export interface RerunSource {
  runId: string
  pipelineId: string
  createdAtMs: number
}

/**
 * Hand-off from the publish preview back to the pipeline editor (IXD-24): the preflight issues
 * to mark on step fields, and the step to select and focus. The editor clears the issues once
 * the draft changes, since only a new preview can tell whether they are fixed.
 */
export const usePreviewStore = defineStore('preview', () => {
  const stepIssues = shallowRef<Issue[]>([])
  const focusStepId = ref<string | null>(null)
  /** "Run again" from P07: the pipeline page opens the preview compared with this run (FLOW-07). */
  const rerunFrom = ref<RerunSource | null>(null)

  function jumpToStep(issues: Issue[], stepId: string) {
    stepIssues.value = issues.filter((i) => i.object?.kind === 'step')
    focusStepId.value = stepId
  }

  function clear() {
    stepIssues.value = []
  }

  return { stepIssues, focusStepId, rerunFrom, jumpToStep, clear }
})

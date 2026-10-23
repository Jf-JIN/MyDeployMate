import { defineStore } from 'pinia'
import { ref, shallowRef } from 'vue'

import type { Issue } from '@/ipc'

/**
 * Hand-off from the publish preview back to the pipeline editor (IXD-24): the preflight issues
 * to mark on step fields, and the step to select and focus. The editor clears the issues once
 * the draft changes, since only a new preview can tell whether they are fixed.
 */
export const usePreviewStore = defineStore('preview', () => {
  const stepIssues = shallowRef<Issue[]>([])
  const focusStepId = ref<string | null>(null)

  function jumpToStep(issues: Issue[], stepId: string) {
    stepIssues.value = issues.filter((i) => i.object?.kind === 'step')
    focusStepId.value = stepId
  }

  function clear() {
    stepIssues.value = []
  }

  return { stepIssues, focusStepId, jumpToStep, clear }
})

<script setup lang="ts">
import { KeyRound, LoaderCircle, PauseCircle } from '@lucide/vue'
import { computed } from 'vue'

import { useT } from '@/i18n'
import { useRunStore } from '@/stores/run'

const t = useT()

// IXD-02: while a run is active the top bar keeps its state and a way back to it on every page;
// leaving a page never cancels the run.
const run = useRunStore()

const view = computed(() => {
  const active = run.active
  if (!active) return null
  if (active.waiting === 'confirm')
    return { icon: PauseCircle, text: t('runChip.waitingConfirm'), tone: 'warning' }
  if (active.waiting === 'credential')
    return { icon: KeyRound, text: t('runChip.waitingCredential'), tone: 'warning' }
  if (active.waiting === 'trust')
    return { icon: KeyRound, text: t('runChip.waitingTrust'), tone: 'warning' }
  if (active.stopping) return { icon: LoaderCircle, text: t('runChip.stopping'), tone: 'warning' }
  return { icon: LoaderCircle, text: t('runChip.running'), tone: 'running' }
})
</script>

<template>
  <RouterLink
    v-if="run.active && view"
    class="run-chip"
    :class="`run-chip--${view.tone}`"
    :to="{ name: 'run', params: { runId: run.active.runId } }"
    :title="`${run.active.projectName} · ${run.active.pipelineName} · ${run.active.environmentName}`"
  >
    <component
      :is="view.icon"
      :class="{ spin: view.tone === 'running' }"
      :size="16"
      :stroke-width="2"
      aria-hidden="true"
    />
    <span>{{ view.text }}</span>
    <span class="num">{{ run.active.done }}/{{ run.active.total }}</span>
    <span class="run-chip-project">{{ run.active.projectName }}</span>
  </RouterLink>
</template>

<style scoped>
.run-chip {
  display: inline-flex;
  flex: none;
  align-items: center;
  gap: var(--space-2);
  height: var(--button-height-compact);
  padding: 0 var(--space-3);
  border: 1px solid currentcolor;
  border-radius: var(--radius-control);
  font: var(--font-label);
  text-decoration: none;
}

.run-chip--running {
  background: var(--state-running-bg);
  color: var(--state-running);
}

.run-chip--warning {
  background: var(--state-warning-bg);
  color: var(--state-warning);
}

.run-chip-project {
  max-width: 160px;
  overflow: hidden;
  color: var(--text-primary);
  font: var(--font-body);
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>

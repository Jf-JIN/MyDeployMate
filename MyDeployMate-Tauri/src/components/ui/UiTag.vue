<script setup lang="ts">
import type { Component } from 'vue'

import type { Tone } from './types'

// State colour always comes with text, and an icon where the meaning is a status (NFR-02).
withDefaults(defineProps<{ tone?: Tone; icon?: Component; spin?: boolean }>(), {
  tone: 'neutral',
  icon: undefined,
})
</script>

<template>
  <span class="tag" :class="`tag--${tone}`">
    <component
      :is="icon"
      v-if="icon"
      :class="{ spin }"
      :size="14"
      :stroke-width="2"
      aria-hidden="true"
    />
    <slot />
  </span>
</template>

<style scoped>
.tag {
  display: inline-flex;
  flex: none;
  align-items: center;
  gap: var(--space-1);
  padding: 1px var(--space-2);
  border-radius: var(--radius-control);
  font: var(--font-caption);
  white-space: nowrap;
}

.tag--neutral {
  background: var(--surface-hover);
  color: var(--text-secondary);
}

.tag--success {
  background: var(--state-success-bg);
  color: var(--state-success);
}

.tag--failure {
  background: var(--state-failure-bg);
  color: var(--state-failure);
}

.tag--warning {
  background: var(--state-warning-bg);
  color: var(--state-warning);
}

.tag--running {
  background: var(--state-running-bg);
  color: var(--state-running);
}

.tag--unknown {
  background: var(--state-unknown-bg);
  color: var(--state-unknown);
}
</style>

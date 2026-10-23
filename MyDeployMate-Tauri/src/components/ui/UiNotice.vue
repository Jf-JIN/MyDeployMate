<script setup lang="ts">
import { CircleAlert, CircleCheck, CircleHelp, Info, TriangleAlert, X } from '@lucide/vue'
import { computed } from 'vue'

import { useT } from '@/i18n'

import type { Tone } from './types'

const t = useT()

// Inline notice bar (UI-19). Failures, permission denials, identity changes and unknown results
// stay on the page until handled, so only non-critical notices offer a close button.
const props = withDefaults(
  defineProps<{ tone?: Exclude<Tone, 'neutral'>; dismissible?: boolean }>(),
  { tone: 'running' },
)

defineEmits<{ dismiss: [] }>()

const icon = computed(
  () =>
    ({
      success: CircleCheck,
      failure: CircleAlert,
      warning: TriangleAlert,
      running: Info,
      unknown: CircleHelp,
    })[props.tone],
)
const role = computed(() => (props.tone === 'failure' ? 'alert' : 'status'))
</script>

<template>
  <div class="notice" :class="`notice--${tone}`" :role="role">
    <component :is="icon" class="notice-icon" :size="16" :stroke-width="2" aria-hidden="true" />
    <div class="notice-text"><slot /></div>
    <div v-if="$slots.actions" class="notice-actions"><slot name="actions" /></div>
    <button
      v-if="dismissible"
      type="button"
      class="notice-close"
      :aria-label="t('ui.notice.dismiss')"
      :title="t('ui.notice.dismiss')"
      @click="$emit('dismiss')"
    >
      <X :size="16" :stroke-width="1.75" aria-hidden="true" />
    </button>
  </div>
</template>

<style scoped>
.notice {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border: 1px solid currentcolor;
  border-radius: var(--radius-control);
}

.notice-icon {
  flex: none;
  margin-top: 3px;
}

.notice-text {
  flex: 1;
  min-width: 0;
  color: var(--text-primary);
  overflow-wrap: anywhere;
}

.notice-actions {
  display: flex;
  flex: none;
  gap: var(--space-2);
}

.notice-close {
  display: inline-flex;
  flex: none;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: 0;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}

.notice--success {
  background: var(--state-success-bg);
  color: var(--state-success);
}

.notice--failure {
  background: var(--state-failure-bg);
  color: var(--state-failure);
}

.notice--warning {
  background: var(--state-warning-bg);
  color: var(--state-warning);
}

.notice--running {
  background: var(--state-running-bg);
  color: var(--state-running);
}

.notice--unknown {
  background: var(--state-unknown-bg);
  color: var(--state-unknown);
}
</style>

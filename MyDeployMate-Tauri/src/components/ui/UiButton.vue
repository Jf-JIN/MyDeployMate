<script setup lang="ts">
import { LoaderCircle } from '@lucide/vue'
import { computed, useId, type Component } from 'vue'

// UI-15: primary / secondary / danger / ghost, plus tinted tonal (teal) and info (blue) for
// actions that sit next to the primary one; busy keeps the width and blocks repeat clicks.
const props = withDefaults(
  defineProps<{
    variant?: 'primary' | 'secondary' | 'tonal' | 'info' | 'danger' | 'ghost'
    compact?: boolean
    icon?: Component
    busy?: boolean
    /** Why the button is unavailable; disables it and is announced with it (UI-06). */
    disabledReason?: string | null
    disabled?: boolean
    type?: 'button' | 'submit'
  }>(),
  { variant: 'secondary', type: 'button', icon: undefined, disabledReason: null },
)

const emit = defineEmits<{ click: [MouseEvent] }>()

const reasonId = useId()
// A button with a reason stays focusable (aria-disabled) so keyboard users can reach the reason.
const blocked = computed(() => props.busy || !!props.disabledReason)

function onClick(event: MouseEvent) {
  if (blocked.value) {
    event.preventDefault()
    return
  }
  emit('click', event)
}
</script>

<template>
  <button
    :type="type"
    class="btn"
    :class="[`btn--${variant}`, { 'btn--compact': compact, 'btn--busy': busy }]"
    :disabled="disabled"
    :aria-disabled="disabledReason ? true : undefined"
    :aria-busy="busy || undefined"
    :aria-describedby="disabledReason ? reasonId : undefined"
    :title="disabledReason ?? undefined"
    @click="onClick"
  >
    <LoaderCircle v-if="busy" class="spin" :size="16" :stroke-width="1.75" aria-hidden="true" />
    <component :is="icon" v-else-if="icon" :size="16" :stroke-width="1.75" aria-hidden="true" />
    <slot />
    <span v-if="disabledReason" :id="reasonId" class="sr-only">{{ disabledReason }}</span>
  </button>
</template>

<style scoped>
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  height: var(--button-height);
  padding: 0 var(--button-padding-x);
  border: 1px solid var(--border-control);
  border-radius: var(--radius-control);
  background: var(--surface-panel);
  color: var(--text-primary);
  font: var(--font-label);
  white-space: nowrap;
  cursor: pointer;
  transition:
    background-color var(--motion-fast),
    border-color var(--motion-fast);
}

.btn--compact {
  height: var(--button-height-compact);
}

.btn:hover:not(:disabled, [aria-disabled='true']) {
  background: var(--surface-hover);
}

.btn--primary {
  border-color: var(--action-primary);
  background: var(--action-primary);
  color: var(--text-on-action);
}

.btn--primary:hover:not(:disabled, [aria-disabled='true']) {
  border-color: var(--action-hover);
  background: var(--action-hover);
}

.btn--primary:active:not(:disabled, [aria-disabled='true']) {
  background: var(--action-pressed);
}

.btn--tonal {
  border-color: color-mix(in srgb, var(--action-primary) 45%, transparent);
  background: var(--surface-selected);
  color: var(--action-primary);
}

.btn--tonal:hover:not(:disabled, [aria-disabled='true']) {
  background: color-mix(in srgb, var(--action-primary) 22%, var(--surface-panel));
}

.btn--info {
  border-color: color-mix(in srgb, var(--state-running) 45%, transparent);
  background: var(--state-running-bg);
  color: var(--state-running);
}

.btn--info:hover:not(:disabled, [aria-disabled='true']) {
  background: color-mix(in srgb, var(--state-running) 18%, var(--surface-panel));
}

.btn--danger {
  border-color: var(--state-failure);
  background: var(--state-failure);
  color: var(--text-on-action);
}

.btn--danger:hover:not(:disabled, [aria-disabled='true']) {
  background: var(--state-failure);
  filter: brightness(0.92);
}

.btn--ghost {
  border-color: transparent;
  background: transparent;
}

.btn:disabled,
.btn[aria-disabled='true'] {
  border-color: var(--border-subtle);
  background: var(--surface-hover);
  color: var(--text-secondary);
  cursor: not-allowed;
}

.btn--busy {
  cursor: progress;
}
</style>

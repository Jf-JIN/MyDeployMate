<script setup lang="ts" generic="T extends string">
import { ref } from 'vue'

import type { SelectOption } from './types'

// UI-18: segments keep a fixed width (the longest label), so the selection never shifts.
// Behaves as a radio group: one tab stop, arrow keys move and select (UI-30).
const model = defineModel<T>({ required: true })

const props = defineProps<{
  options: SelectOption<T>[]
  label: string
  disabled?: boolean
}>()

const buttons = ref<HTMLButtonElement[]>([])

function enabled(option: SelectOption<T>) {
  return !props.disabled && !option.disabledReason
}

function select(option: SelectOption<T>) {
  if (enabled(option)) model.value = option.value
}

function onKeydown(event: KeyboardEvent, index: number) {
  const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[event.key]
  if (!step) return
  event.preventDefault()
  const count = props.options.length
  for (let i = 1; i < count; i++) {
    const next = (index + step * i + count) % count
    const option = props.options[next]
    if (option && enabled(option)) {
      model.value = option.value
      buttons.value[next]?.focus()
      return
    }
  }
}
</script>

<template>
  <div class="segmented" role="radiogroup" :aria-label="label">
    <button
      v-for="(option, index) in options"
      :key="option.value"
      ref="buttons"
      type="button"
      role="radio"
      class="segment"
      :aria-checked="model === option.value"
      :aria-disabled="!enabled(option) || undefined"
      :title="option.disabledReason"
      :tabindex="model === option.value ? 0 : -1"
      @click="select(option)"
      @keydown="onKeydown($event, index)"
    >
      <span class="segment-label" :data-label="option.label">{{ option.label }}</span>
    </button>
  </div>
</template>

<style scoped>
.segmented {
  display: inline-flex;
  flex: none;
  align-self: flex-start;
  overflow: hidden;
  border: 1px solid var(--border-control);
  border-radius: var(--radius-control);
  background: var(--surface-panel);
}

.segment {
  height: 30px;
  padding: 0 var(--space-3);
  border: 0;
  background: transparent;
  font: var(--font-body);
  color: var(--text-primary);
  cursor: pointer;
}

.segment + .segment {
  border-left: 1px solid var(--border-control);
}

/* Reserve the bold width so selecting a segment does not move its neighbours. */
.segment-label {
  display: inline-grid;
}

.segment-label::after {
  content: attr(data-label);
  height: 0;
  visibility: hidden;
  font-weight: 600;
}

.segment[aria-checked='true'] {
  background: var(--surface-selected);
  color: var(--action-primary);
  font-weight: 600;
}

.segment:hover:not([aria-checked='true'], [aria-disabled='true']) {
  background: var(--surface-hover);
}

.segment[aria-disabled='true'] {
  color: var(--text-secondary);
  cursor: not-allowed;
}

.segment:focus-visible {
  outline-offset: -2px;
}
</style>

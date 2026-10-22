<script setup lang="ts" generic="T extends string | number">
// UI-16 / IXD-04: a native select, so keyboard and screen readers behave as on Windows.
// Disabled options carry their reason in the label (UI-18).
import type { SelectOption } from './types'

const model = defineModel<T>({ required: true })

withDefaults(
  defineProps<{
    options: SelectOption<T>[]
    id?: string
    invalid?: boolean
    disabled?: boolean
    compact?: boolean
    describedBy?: string
  }>(),
  { id: undefined, describedBy: undefined },
)
</script>

<template>
  <div class="select" :class="{ 'select--compact': compact }">
    <select
      :id="id"
      v-model="model"
      class="control"
      :disabled="disabled"
      :aria-invalid="invalid || undefined"
      :aria-describedby="describedBy"
    >
      <option
        v-for="option in options"
        :key="String(option.value)"
        :value="option.value"
        :disabled="!!option.disabledReason"
      >
        {{ option.disabledReason ? `${option.label}（${option.disabledReason}）` : option.label }}
      </option>
    </select>
  </div>
</template>

<style scoped>
.select {
  position: relative;
  min-width: 0;
}

.select::after {
  content: '';
  position: absolute;
  top: 50%;
  right: 14px;
  width: 7px;
  height: 7px;
  margin-top: -6px;
  border-right: 1.75px solid var(--text-secondary);
  border-bottom: 1.75px solid var(--text-secondary);
  transform: rotate(45deg);
  pointer-events: none;
}

.select .control {
  padding-right: 36px;
  appearance: none;
  cursor: pointer;
}

.select--compact .control {
  height: var(--button-height-compact);
}
</style>

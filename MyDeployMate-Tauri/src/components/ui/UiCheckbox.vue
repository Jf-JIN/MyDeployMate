<script setup lang="ts">
// IXD-04: multi-select such as target servers. Native checkbox for keyboard and AT support.
const model = defineModel<boolean>({ required: true })

withDefaults(
  defineProps<{
    label?: string
    /** Hide the visible label but keep it as the accessible name (table rows). */
    hideLabel?: boolean
    indeterminate?: boolean
    disabled?: boolean
  }>(),
  { label: undefined },
)
</script>

<template>
  <label class="checkbox" :class="{ 'checkbox--disabled': disabled }">
    <input
      v-model="model"
      type="checkbox"
      :indeterminate="indeterminate"
      :disabled="disabled"
      :aria-label="hideLabel ? label : undefined"
    />
    <span v-if="label && !hideLabel">{{ label }}</span>
    <slot />
  </label>
</template>

<style scoped>
.checkbox {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  cursor: pointer;
}

.checkbox input {
  width: 16px;
  height: 16px;
  margin: 0;
  accent-color: var(--action-primary);
  cursor: inherit;
}

.checkbox--disabled {
  color: var(--text-secondary);
  cursor: not-allowed;
}
</style>

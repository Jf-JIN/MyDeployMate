<script setup lang="ts">
import { computed, useId } from 'vue'

// UI-16 / UI-19: label above the control, hint and error below; the error is tied to the
// control with aria-describedby and never clears the value.
const props = defineProps<{
  label: string
  hint?: string
  error?: string | null
  required?: boolean
  /** Use a fieldset/legend for groups such as segmented controls and radio sets. */
  group?: boolean
}>()

const id = useId()
const hintId = `${id}-hint`
const errorId = `${id}-error`

const describedBy = computed(
  () =>
    [props.error ? errorId : null, props.hint ? hintId : null].filter(Boolean).join(' ') ||
    undefined,
)
</script>

<template>
  <component :is="group ? 'fieldset' : 'div'" class="field" :class="{ 'field--invalid': !!error }">
    <component :is="group ? 'legend' : 'label'" class="field-label" :for="group ? undefined : id">
      {{ label }}<span v-if="required" class="required" aria-hidden="true">*</span>
    </component>
    <slot :id="id" :described-by="describedBy" :invalid="!!error" />
    <p v-if="error" :id="errorId" class="field-error">{{ error }}</p>
    <p v-if="hint" :id="hintId" class="field-hint">{{ hint }}</p>
  </component>
</template>

<style scoped>
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
  margin: 0;
  padding: 0;
  border: 0;
}

.field-label {
  padding: 0;
  font: var(--font-label);
}

.required {
  margin-left: 2px;
  color: var(--state-failure);
}

.field-error,
.field-hint {
  margin: 0;
  font: var(--font-caption);
}

.field-error {
  color: var(--state-failure);
}

.field-hint {
  color: var(--text-secondary);
}
</style>

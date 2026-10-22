<script setup lang="ts">
// UI-16: ports and timeouts use a stepper. Out-of-range values are kept as typed and reported
// by Rust validation; the field never clamps or clears them.
const model = defineModel<number>({ required: true })

withDefaults(
  defineProps<{
    id?: string
    min?: number
    max?: number
    step?: number
    invalid?: boolean
    readonly?: boolean
    disabled?: boolean
    describedBy?: string
    /** Accessible name when no visible label points at the control (table cells). */
    ariaLabel?: string
  }>(),
  {
    id: undefined,
    min: undefined,
    max: undefined,
    step: 1,
    describedBy: undefined,
    ariaLabel: undefined,
  },
)

function onInput(event: Event) {
  const value = (event.target as HTMLInputElement).valueAsNumber
  if (!Number.isNaN(value)) model.value = value
}
</script>

<template>
  <input
    :id="id"
    class="control num"
    type="number"
    :value="model"
    :min="min"
    :max="max"
    :step="step"
    :readonly="readonly"
    :disabled="disabled"
    :aria-invalid="invalid || undefined"
    :aria-describedby="describedBy"
    :aria-label="ariaLabel"
    @input="onInput"
  />
</template>

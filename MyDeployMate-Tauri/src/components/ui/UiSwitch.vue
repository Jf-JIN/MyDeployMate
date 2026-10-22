<script setup lang="ts">
// IXD-04 / UI-30: on/off settings such as "enabled" and "protected"; Space toggles.
const model = defineModel<boolean>({ required: true })

withDefaults(
  defineProps<{
    /** Accessible name when no visible label is linked by id. */
    label?: string
    id?: string
    disabled?: boolean
    describedBy?: string
  }>(),
  { label: undefined, id: undefined, describedBy: undefined },
)
</script>

<template>
  <button
    :id="id"
    type="button"
    role="switch"
    class="switch"
    :aria-checked="model"
    :aria-label="label"
    :title="label"
    :aria-describedby="describedBy"
    :disabled="disabled"
    @click="model = !model"
  >
    <span class="knob" aria-hidden="true" />
  </button>
</template>

<style scoped>
.switch {
  position: relative;
  flex: none;
  width: 36px;
  height: 20px;
  padding: 0;
  border: 1px solid var(--border-control);
  border-radius: 10px;
  background: var(--surface-panel);
  cursor: pointer;
  transition: background-color var(--motion-fast);
}

.knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--border-control);
  transition: transform var(--motion-fast);
}

.switch[aria-checked='true'] {
  border-color: var(--action-primary);
  background: var(--action-primary);
}

.switch[aria-checked='true'] .knob {
  background: var(--surface-panel);
  transform: translateX(16px);
}

.switch:disabled {
  border-color: var(--border-subtle);
  background: var(--surface-hover);
  cursor: not-allowed;
}

.switch:disabled .knob {
  background: var(--border-subtle);
}
</style>

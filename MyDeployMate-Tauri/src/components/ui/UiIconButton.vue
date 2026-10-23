<script setup lang="ts">
import { computed, useId, type Component } from 'vue'

// UI-08: icon-only actions always carry an accessible name and a tooltip.
const props = withDefaults(
  defineProps<{
    icon: Component
    label: string
    /** 36 px hit area for main toolbars, 32 px otherwise. */
    toolbar?: boolean
    disabledReason?: string | null
    pressed?: boolean | null
    /** Field-side action (browse): input-height, tinted like the insert-variable button. */
    tonal?: boolean
  }>(),
  { disabledReason: null, pressed: null },
)

const emit = defineEmits<{ click: [MouseEvent] }>()

const reasonId = useId()
const tooltip = computed(() =>
  props.disabledReason ? `${props.label}（${props.disabledReason}）` : props.label,
)

function onClick(event: MouseEvent) {
  if (props.disabledReason) return
  emit('click', event)
}
</script>

<template>
  <button
    type="button"
    class="icon-btn"
    :class="{ 'icon-btn--toolbar': toolbar, 'icon-btn--tonal': tonal }"
    :aria-label="label"
    :aria-disabled="disabledReason ? true : undefined"
    :aria-describedby="disabledReason ? reasonId : undefined"
    :aria-pressed="pressed ?? undefined"
    :title="tooltip"
    @click="onClick"
  >
    <component :is="icon" :size="16" :stroke-width="1.75" aria-hidden="true" />
    <span v-if="disabledReason" :id="reasonId" class="sr-only">{{ disabledReason }}</span>
  </button>
</template>

<style scoped>
.icon-btn {
  display: inline-flex;
  flex: none;
  align-items: center;
  justify-content: center;
  width: var(--icon-button-size);
  height: var(--icon-button-size);
  padding: 0;
  border: 0;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: background-color var(--motion-fast);
}

.icon-btn--toolbar {
  width: var(--icon-button-size-toolbar);
  height: var(--icon-button-size-toolbar);
}

.icon-btn--tonal {
  width: var(--input-height);
  height: var(--input-height);
  border: 1px solid color-mix(in srgb, var(--action-primary) 45%, transparent);
  background: var(--surface-selected);
  color: var(--action-primary);
}

.icon-btn:hover:not([aria-disabled='true']) {
  background: var(--surface-hover);
  color: var(--text-primary);
}

.icon-btn--tonal:hover:not([aria-disabled='true']) {
  background: color-mix(in srgb, var(--action-primary) 22%, var(--surface-panel));
  color: var(--action-primary);
}

.icon-btn[aria-pressed='true'] {
  background: var(--surface-selected);
  color: var(--action-primary);
}

.icon-btn[aria-disabled='true'] {
  border-color: var(--border-subtle);
  background: transparent;
  color: var(--border-control);
  cursor: not-allowed;
}
</style>

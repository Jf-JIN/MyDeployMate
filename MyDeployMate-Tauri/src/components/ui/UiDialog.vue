<script setup lang="ts">
import { X } from '@lucide/vue'
import { nextTick, onBeforeUnmount, ref, useId, watch } from 'vue'

import { focusInitial, isTopLayer, popLayer, pushLayer, trapTab } from '@/composables/focus'

// UI-14 / IXD-03: modal with fixed header and footer and a scrolling body. Focus moves to the
// first field on open, stays inside while open, and returns to the trigger on close.
// Esc and the close button emit `cancel`; the owner decides (for example after a dirty check).
// Clicking the backdrop does nothing unless `backdropCancels` is set, so trust, publish,
// password and cancel-run dialogs cannot be dismissed by accident.
const props = withDefaults(
  defineProps<{
    open: boolean
    title: string
    description?: string
    width?: 'default' | 'server' | 'preview' | 'credential'
    backdropCancels?: boolean
    /** Set false while a step is in flight so Esc cannot interrupt it. */
    cancellable?: boolean
  }>(),
  { description: undefined, width: 'default', cancellable: true },
)

const emit = defineEmits<{ cancel: [] }>()

const titleId = useId()
const descriptionId = useId()
const panel = ref<HTMLElement | null>(null)
const body = ref<HTMLElement | null>(null)

let layer: symbol | null = null
let trigger: HTMLElement | null = null

function activate() {
  trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null
  layer = pushLayer()
  void nextTick(() => {
    if (panel.value) focusInitial(panel.value, body.value)
  })
}

function deactivate() {
  if (!layer) return
  popLayer(layer)
  layer = null
  if (trigger?.isConnected) trigger.focus()
  trigger = null
}

watch(
  () => props.open,
  (open) => (open ? activate() : deactivate()),
  { immediate: true },
)
onBeforeUnmount(deactivate)

function requestCancel() {
  if (props.cancellable) emit('cancel')
}

function onKeydown(event: KeyboardEvent) {
  if (!layer || !isTopLayer(layer) || !panel.value) return
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    requestCancel()
    return
  }
  trapTab(panel.value, event)
}

function onBackdrop(event: MouseEvent) {
  if (props.backdropCancels && event.target === event.currentTarget) requestCancel()
}
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="backdrop" @mousedown="onBackdrop" @keydown="onKeydown">
      <div
        ref="panel"
        class="dialog"
        :class="`dialog--${width}`"
        role="dialog"
        aria-modal="true"
        :aria-labelledby="titleId"
        :aria-describedby="description ? descriptionId : undefined"
        tabindex="-1"
      >
        <header class="dialog-header">
          <div class="dialog-heading">
            <h2 :id="titleId" class="dialog-title">{{ title }}</h2>
            <p v-if="description" :id="descriptionId" class="dialog-description">
              {{ description }}
            </p>
          </div>
          <button
            v-if="cancellable"
            type="button"
            class="dialog-close"
            aria-label="关闭"
            title="关闭"
            @click="requestCancel"
          >
            <X :size="18" :stroke-width="1.75" aria-hidden="true" />
          </button>
        </header>
        <div ref="body" class="dialog-body">
          <slot />
        </div>
        <footer v-if="$slots.footer" class="dialog-footer">
          <slot name="footer" />
        </footer>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgb(32 36 40 / 40%);
}

.dialog {
  display: flex;
  flex-direction: column;
  width: var(--dialog-width);
  max-width: calc(100vw - 48px);
  max-height: calc(100vh - 64px);
  border-radius: var(--radius-dialog);
  background: var(--surface-panel);
  box-shadow: 0 8px 32px rgb(32 36 40 / 20%);
  outline: none;
  animation: dialog-in var(--motion-panel) ease-out;
}

.dialog--server {
  width: var(--dialog-width-server);
}

.dialog--preview {
  width: var(--dialog-width-preview);
}

.dialog--credential {
  width: var(--dialog-width-credential);
}

.dialog-header {
  display: flex;
  flex: none;
  align-items: flex-start;
  gap: var(--space-3);
  padding: var(--space-4) var(--space-4) var(--space-3) var(--space-5);
  border-bottom: 1px solid var(--border-subtle);
}

.dialog-heading {
  flex: 1;
  min-width: 0;
}

.dialog-title {
  margin: 0;
  font: var(--font-dialog-title);
  overflow-wrap: anywhere;
}

.dialog-description {
  margin: var(--space-1) 0 0;
  color: var(--text-secondary);
}

.dialog-close {
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
}

.dialog-close:hover {
  background: var(--surface-hover);
}

/* Padding keeps focus rings and error borders inside the scroll area (UI-14). */
.dialog-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: var(--space-4) var(--space-5);
}

.dialog-footer {
  display: flex;
  flex: none;
  justify-content: flex-end;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-5);
  border-top: 1px solid var(--border-subtle);
}

@keyframes dialog-in {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
}
</style>

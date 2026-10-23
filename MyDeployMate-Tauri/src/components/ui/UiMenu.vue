<script setup lang="ts">
import { ChevronDown } from '@lucide/vue'
import { computed, nextTick, onBeforeUnmount, ref, useId, type Component } from 'vue'

import type { MenuEntry } from './types'

// UI-18: a menu button. Items are at least 32 px, dangerous ones sit in their own group, and a
// disabled item says why. Arrow keys move, Enter selects, Esc closes and returns focus.
const props = withDefaults(
  defineProps<{
    label: string
    entries: MenuEntry[]
    icon?: Component | null
    /** Show only the icon; `label` stays the accessible name. */
    iconOnly?: boolean
    compact?: boolean
    /** Tinted, bordered trigger the height of a text field, for actions attached to a field. */
    tonal?: boolean
    align?: 'start' | 'end'
  }>(),
  { icon: null, align: 'start' },
)

const emit = defineEmits<{ select: [key: string] }>()

const open = ref(false)
const root = ref<HTMLElement | null>(null)
const trigger = ref<HTMLButtonElement | null>(null)
const items = ref<HTMLButtonElement[]>([])
const menuId = useId()

const selectable = computed(() =>
  props.entries.filter((e) => e.kind !== 'heading' && !e.disabledReason),
)

function onOutside(event: PointerEvent) {
  if (!root.value?.contains(event.target as Node)) close(false)
}

async function show() {
  open.value = true
  document.addEventListener('pointerdown', onOutside, true)
  await nextTick()
  focusIndex(0)
}

function close(returnFocus = true) {
  open.value = false
  document.removeEventListener('pointerdown', onOutside, true)
  if (returnFocus) trigger.value?.focus()
}

onBeforeUnmount(() => document.removeEventListener('pointerdown', onOutside, true))

function focusIndex(i: number) {
  const enabled = items.value.filter((el) => el.getAttribute('aria-disabled') !== 'true')
  const n = enabled.length
  if (!n) return
  enabled[((i % n) + n) % n]?.focus()
}

function currentIndex() {
  const enabled = items.value.filter((el) => el.getAttribute('aria-disabled') !== 'true')
  return enabled.indexOf(document.activeElement as HTMLButtonElement)
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    close()
  } else if (event.key === 'ArrowDown') {
    event.preventDefault()
    focusIndex(currentIndex() + 1)
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    focusIndex(currentIndex() - 1)
  } else if (event.key === 'Home') {
    event.preventDefault()
    focusIndex(0)
  } else if (event.key === 'End') {
    event.preventDefault()
    focusIndex(selectable.value.length - 1)
  } else if (event.key === 'Tab') {
    close(false)
  }
}

function choose(entry: MenuEntry) {
  if (entry.kind === 'heading' || entry.disabledReason) return
  close()
  emit('select', entry.key)
}

function onTriggerKeydown(event: KeyboardEvent) {
  if (event.key === 'ArrowDown' && !open.value) {
    event.preventDefault()
    void show()
  }
}
</script>

<template>
  <div ref="root" class="menu-root">
    <button
      ref="trigger"
      type="button"
      class="menu-trigger"
      :class="{
        'menu-trigger--icon': iconOnly,
        'menu-trigger--compact': compact,
        'menu-trigger--tonal': tonal,
      }"
      aria-haspopup="menu"
      :aria-expanded="open"
      :aria-controls="open ? menuId : undefined"
      :aria-label="iconOnly ? label : undefined"
      :title="iconOnly ? label : undefined"
      @click="open ? close() : show()"
      @keydown="onTriggerKeydown"
    >
      <component :is="icon" v-if="icon" :size="16" :stroke-width="1.75" aria-hidden="true" />
      <template v-if="!iconOnly">
        <span>{{ label }}</span>
        <ChevronDown :size="14" :stroke-width="1.75" aria-hidden="true" />
      </template>
    </button>
    <div
      v-if="open"
      :id="menuId"
      class="menu"
      :class="`menu--${align}`"
      role="menu"
      :aria-label="label"
      @keydown="onKeydown"
    >
      <template v-for="entry in entries" :key="entry.key">
        <div v-if="entry.kind === 'heading'" class="menu-heading" role="presentation">
          {{ entry.label }}
        </div>
        <template v-else>
          <div v-if="entry.separator" class="menu-separator" role="separator" />
          <button
            ref="items"
            type="button"
            role="menuitem"
            class="menu-item"
            :class="{ 'menu-item--danger': entry.danger }"
            :aria-disabled="entry.disabledReason ? true : undefined"
            :title="entry.disabledReason ?? undefined"
            tabindex="-1"
            @click="choose(entry)"
          >
            <component
              :is="entry.icon"
              v-if="entry.icon"
              :size="16"
              :stroke-width="1.75"
              aria-hidden="true"
            />
            <span class="menu-label">{{ entry.label }}</span>
            <span v-if="entry.detail" class="menu-detail">{{ entry.detail }}</span>
            <span v-if="entry.disabledReason" class="menu-detail">{{ entry.disabledReason }}</span>
          </button>
        </template>
      </template>
    </div>
  </div>
</template>

<style scoped>
.menu-root {
  position: relative;
  display: inline-flex;
}

.menu-trigger {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  height: var(--button-height);
  padding: 0 var(--space-3);
  border: 1px solid var(--border-control);
  border-radius: var(--radius-control);
  background: var(--surface-panel);
  color: var(--text-primary);
  font: var(--font-body);
  cursor: pointer;
}

.menu-trigger--compact {
  height: var(--button-height-compact);
}

.menu-trigger--icon {
  justify-content: center;
  width: var(--icon-button-size);
  height: var(--icon-button-size);
  padding: 0;
  border-color: transparent;
  background: transparent;
  color: var(--text-secondary);
}

.menu-trigger--tonal {
  width: var(--input-height);
  height: var(--input-height);
  border-color: color-mix(in srgb, var(--action-primary) 45%, transparent);
  background: var(--surface-selected);
  color: var(--action-primary);
}

.menu-trigger:hover {
  background: var(--surface-hover);
}

.menu-trigger--tonal:hover,
.menu-trigger--tonal[aria-expanded='true'] {
  background: color-mix(in srgb, var(--action-primary) 22%, var(--surface-panel));
}

.menu {
  position: absolute;
  top: calc(100% + 4px);
  z-index: 30;
  min-width: 200px;
  max-height: 360px;
  overflow-y: auto;
  padding: var(--space-1) 0;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-control);
  background: var(--surface-panel);
  box-shadow: var(--shadow-popover);
}

.menu--start {
  left: 0;
}

.menu--end {
  right: 0;
}

.menu-heading {
  padding: var(--space-2) var(--space-3) var(--space-1);
  color: var(--text-secondary);
  font: var(--font-caption);
}

.menu-separator {
  height: 1px;
  margin: var(--space-1) 0;
  background: var(--border-subtle);
}

.menu-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  min-height: 32px;
  padding: 0 var(--space-3);
  border: 0;
  background: transparent;
  color: var(--text-primary);
  font: var(--font-body);
  text-align: left;
  white-space: nowrap;
  cursor: pointer;
}

.menu-item:hover:not([aria-disabled='true']),
.menu-item:focus-visible {
  background: var(--surface-hover);
  outline: none;
}

.menu-item[aria-disabled='true'] {
  color: var(--text-secondary);
  cursor: not-allowed;
}

.menu-item--danger {
  color: var(--state-failure);
}

.menu-label {
  flex: 1;
}

.menu-detail {
  color: var(--text-secondary);
  font: var(--font-caption);
}
</style>

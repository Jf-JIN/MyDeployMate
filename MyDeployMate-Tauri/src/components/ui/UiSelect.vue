<script setup lang="ts" generic="T extends string | number">
// UI-16 / UI-18 / IXD-04: a select-only combobox. Focus stays on the trigger and the open list
// follows with aria-activedescendant, as the Windows select does: arrows move, Enter or Space
// picks, Esc closes, typing jumps to a matching option. The list is teleported to <body> so
// dialogs and scroll areas cannot clip it. Disabled options carry their reason (UI-18).
import { Check } from '@lucide/vue'
import { computed, nextTick, onBeforeUnmount, ref, useId } from 'vue'

import type { SelectOption } from './types'

const model = defineModel<T>({ required: true })

const props = withDefaults(
  defineProps<{
    options: SelectOption<T>[]
    id?: string
    invalid?: boolean
    disabled?: boolean
    compact?: boolean
    describedBy?: string
    /** Accessible name when no visible label points at the control (table cells). */
    ariaLabel?: string
  }>(),
  { id: undefined, describedBy: undefined, ariaLabel: undefined },
)

const listId = useId()
const trigger = ref<HTMLButtonElement | null>(null)
const list = ref<HTMLElement | null>(null)
const open = ref(false)
const active = ref(-1)
const place = ref<{ left: number; width: number; top?: number; bottom?: number }>({
  left: 0,
  width: 0,
})

const selectedIndex = computed(() => props.options.findIndex((o) => o.value === model.value))
const selectedLabel = computed(() => props.options[selectedIndex.value]?.label ?? '')

function optionId(i: number) {
  return `${listId}-${i}`
}

function enabled(i: number) {
  const option = props.options[i]
  return !!option && !option.disabledReason
}

/** The next enabled option from `from` in `step` direction, or `from` when there is none. */
function step(from: number, delta: number): number {
  for (let i = from + delta; i >= 0 && i < props.options.length; i += delta) {
    if (enabled(i)) return i
  }
  return from
}

const LIST_MAX = 280

function position() {
  const rect = trigger.value?.getBoundingClientRect()
  if (!rect) return
  const below = window.innerHeight - rect.bottom
  const up = below < Math.min(LIST_MAX, props.options.length * 32 + 8) + 8 && rect.top > below
  place.value = up
    ? { left: rect.left, width: rect.width, bottom: window.innerHeight - rect.top + 4 }
    : { left: rect.left, width: rect.width, top: rect.bottom + 4 }
}

function onOutside(event: PointerEvent) {
  const target = event.target as Node
  if (!trigger.value?.contains(target) && !list.value?.contains(target)) close()
}

function onScroll(event: Event) {
  if (!list.value?.contains(event.target as Node)) position()
}

async function show() {
  if (props.disabled || open.value) return
  position()
  open.value = true
  active.value = enabled(selectedIndex.value) ? selectedIndex.value : step(-1, 1)
  document.addEventListener('pointerdown', onOutside, true)
  window.addEventListener('scroll', onScroll, true)
  window.addEventListener('resize', close)
  await nextTick()
  reveal()
}

function close() {
  open.value = false
  document.removeEventListener('pointerdown', onOutside, true)
  window.removeEventListener('scroll', onScroll, true)
  window.removeEventListener('resize', close)
}

onBeforeUnmount(close)

function reveal() {
  const box = list.value
  const item = active.value < 0 ? null : document.getElementById(optionId(active.value))
  if (!box || !item) return
  // The list is position: fixed, so it is the option's offsetParent.
  if (item.offsetTop < box.scrollTop) box.scrollTop = item.offsetTop
  else if (item.offsetTop + item.offsetHeight > box.scrollTop + box.clientHeight) {
    box.scrollTop = item.offsetTop + item.offsetHeight - box.clientHeight
  }
}

function pick(i: number) {
  if (!enabled(i)) return
  model.value = props.options[i]!.value
  close()
}

function moveTo(i: number) {
  active.value = i
  void nextTick(reveal)
}

let typed = ''
let typedAt = 0

/** Type-ahead: jump to the first enabled option whose label starts with what was typed. */
function typeahead(key: string): number {
  const now = Date.now()
  typed = now - typedAt > 700 ? key : typed + key
  typedAt = now
  const query = typed.toLowerCase()
  const start = open.value ? active.value : selectedIndex.value
  const n = props.options.length
  for (let k = 1; k <= n; k++) {
    const i = (start + (typed.length > 1 ? k - 1 : k) + n) % n
    if (enabled(i) && props.options[i]!.label.toLowerCase().startsWith(query)) return i
  }
  return -1
}

function onKeydown(event: KeyboardEvent) {
  const key = event.key
  if (!open.value) {
    if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(key)) {
      event.preventDefault()
      void show()
    } else if (key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      const i = typeahead(key)
      if (i >= 0) model.value = props.options[i]!.value
    }
    return
  }
  if (key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    close()
  } else if (key === 'Tab') {
    close()
  } else if (key === 'ArrowDown') {
    event.preventDefault()
    moveTo(step(active.value, 1))
  } else if (key === 'ArrowUp') {
    event.preventDefault()
    moveTo(step(active.value, -1))
  } else if (key === 'Home') {
    event.preventDefault()
    moveTo(step(-1, 1))
  } else if (key === 'End') {
    event.preventDefault()
    moveTo(step(props.options.length, -1))
  } else if (key === 'Enter' || key === ' ') {
    event.preventDefault()
    pick(active.value)
  } else if (key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
    const i = typeahead(key)
    if (i >= 0) moveTo(i)
  }
}

const listStyle = computed(() => ({
  left: `${place.value.left}px`,
  minWidth: `${place.value.width}px`,
  maxWidth: `max(${place.value.width}px, 420px)`,
  top: place.value.top === undefined ? undefined : `${place.value.top}px`,
  bottom: place.value.bottom === undefined ? undefined : `${place.value.bottom}px`,
}))
</script>

<template>
  <div class="select" :class="{ 'select--compact': compact }">
    <button
      :id="id"
      ref="trigger"
      type="button"
      class="control select-trigger"
      role="combobox"
      aria-haspopup="listbox"
      :aria-expanded="open"
      :aria-controls="open ? listId : undefined"
      :aria-activedescendant="open && active >= 0 ? optionId(active) : undefined"
      :disabled="disabled"
      :aria-invalid="invalid || undefined"
      :aria-describedby="describedBy"
      :aria-label="ariaLabel"
      @click="open ? close() : show()"
      @keydown="onKeydown"
    >
      <span class="select-value">{{ selectedLabel }}</span>
    </button>
    <Teleport to="body">
      <ul
        v-if="open"
        :id="listId"
        ref="list"
        class="select-list"
        role="listbox"
        :aria-label="ariaLabel"
        :style="listStyle"
        @mousedown.prevent
      >
        <li
          v-for="(option, i) in options"
          :id="optionId(i)"
          :key="String(option.value)"
          role="option"
          class="select-option"
          :class="{ 'select-option--active': i === active }"
          :aria-selected="i === selectedIndex"
          :aria-disabled="option.disabledReason ? true : undefined"
          @pointermove="enabled(i) && (active = i)"
          @click="pick(i)"
        >
          <span class="select-option-label">{{ option.label }}</span>
          <span v-if="option.disabledReason" class="select-option-reason">
            {{ option.disabledReason }}
          </span>
          <Check
            v-else-if="i === selectedIndex"
            class="select-check"
            :size="14"
            :stroke-width="2"
            aria-hidden="true"
          />
        </li>
      </ul>
    </Teleport>
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

.select-trigger {
  display: flex;
  align-items: center;
  padding-right: 36px;
  text-align: left;
  cursor: pointer;
}

.select-trigger:disabled {
  cursor: not-allowed;
}

.select-value {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.select--compact .control {
  height: var(--button-height-compact);
}

/* Scoped styles still reach the list: Teleport keeps the component's scope attribute. */
.select-list {
  position: fixed;
  z-index: 150;
  max-height: 280px;
  margin: 0;
  padding: var(--space-1);
  overflow-y: auto;
  border: 1px solid var(--border-control);
  border-radius: var(--radius-surface);
  background: var(--surface-popover);
  box-shadow: var(--shadow-popover);
  list-style: none;
}

.select-option {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-height: 32px;
  padding: 0 var(--space-2) 0 var(--space-3);
  border-radius: var(--radius-control);
  color: var(--text-primary);
  font: var(--font-body);
  white-space: nowrap;
  cursor: pointer;
}

.select-option--active {
  background: var(--surface-popover-hover);
}

.select-option[aria-selected='true'] {
  background: var(--surface-selected);
  color: var(--action-primary);
  font-weight: 600;
}

.select-option[aria-selected='true'].select-option--active {
  background: color-mix(in srgb, var(--action-primary) 18%, var(--surface-popover));
}

.select-option[aria-disabled='true'] {
  color: var(--text-secondary);
  cursor: not-allowed;
}

.select-option-label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

.select-option-reason {
  color: var(--text-secondary);
  font: var(--font-caption);
}

.select-check {
  flex: none;
}
</style>

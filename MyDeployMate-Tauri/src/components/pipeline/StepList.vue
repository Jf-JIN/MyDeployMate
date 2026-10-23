<script setup lang="ts">
import {
  ArrowDown,
  ArrowUp,
  CircleAlert,
  Copy,
  Hand,
  Plus,
  Terminal,
  Trash2,
  Upload,
  Monitor,
  type LucideIcon,
} from '@lucide/vue'
import { computed, nextTick, ref, toRaw } from 'vue'

import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiMenu from '@/components/ui/UiMenu.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import UiTag from '@/components/ui/UiTag.vue'
import type { MenuEntry } from '@/components/ui/types'
import { useT } from '@/i18n'
import { call, type Step, type StepKind } from '@/ipc'

import { moveStep, nextSelection } from './pipeline'

const t = useT()

// IXD-09: the step list. Selecting a row only changes the parameter panel; the toolbar acts on
// the selected step. The list edits the page draft in place.
const steps = defineModel<Step[]>('steps', { required: true })
const selectedId = defineModel<string | null>('selectedId', { required: true })
const props = defineProps<{
  /** Whether validation reported anything for the step at this index. */
  hasIssue: (index: number) => boolean
}>()

const KIND_ICON: Record<StepKind, LucideIcon> = {
  local: Monitor,
  upload: Upload,
  remote: Terminal,
  confirm: Hand,
}

const addEntries = computed<MenuEntry[]>(() =>
  (['local', 'upload', 'remote', 'confirm'] as const).map((k) => ({
    key: k,
    label: t(`steps.kind.${k}`),
    icon: KIND_ICON[k],
  })),
)

const index = computed(() => steps.value.findIndex((s) => s.id === selectedId.value))
const selected = computed(() => steps.value[index.value] ?? null)

const list = ref<HTMLElement | null>(null)
async function focusSelected() {
  await nextTick()
  list.value?.querySelector<HTMLElement>('[aria-current="true"]')?.focus()
}

async function add(kind: string) {
  const step = await call('new_step', { kind: kind as StepKind })
  steps.value.push(step)
  selectedId.value = step.id
  await focusSelected()
}

function move(delta: -1 | 1) {
  moveStep(steps.value, index.value, delta)
}

async function copy() {
  if (!selected.value) return
  const copied = await call('copy_step', { step: structuredClone(toRaw(selected.value)) })
  steps.value.splice(index.value + 1, 0, copied)
  selectedId.value = copied.id
  await focusSelected()
}

const deleting = ref(false)
function remove() {
  const i = index.value
  if (i < 0) return
  steps.value.splice(i, 1)
  selectedId.value = nextSelection(steps.value, i)
  deleting.value = false
  void focusSelected()
}

const noSelection = computed(() => (selected.value ? null : t('steps.selectFirst')))
const upReason = computed(() => noSelection.value ?? (index.value === 0 ? t('steps.first') : null))
const downReason = computed(
  () => noSelection.value ?? (index.value === steps.value.length - 1 ? t('steps.last') : null),
)
</script>

<template>
  <section class="step-list" :aria-label="t('steps.title')">
    <div class="toolbar">
      <h2 class="title">{{ t('steps.title') }}</h2>
      <span class="actions">
        <UiIconButton
          :icon="ArrowUp"
          :label="t('steps.moveUp')"
          :disabled-reason="upReason"
          @click="move(-1)"
        />
        <UiIconButton
          :icon="ArrowDown"
          :label="t('steps.moveDown')"
          :disabled-reason="downReason"
          @click="move(1)"
        />
        <UiIconButton
          :icon="Copy"
          :label="t('steps.copy')"
          :disabled-reason="noSelection"
          @click="copy"
        />
        <UiIconButton
          :icon="Trash2"
          :label="t('steps.delete')"
          :disabled-reason="noSelection"
          @click="deleting = true"
        />
      </span>
      <UiMenu
        :label="t('steps.add')"
        :icon="Plus"
        :entries="addEntries"
        compact
        align="end"
        @select="add"
      />
    </div>

    <ol v-if="steps.length" ref="list" class="rows">
      <li
        v-for="(step, i) in steps"
        :key="step.id"
        class="row"
        :class="{ 'is-selected': step.id === selectedId, 'is-disabled': !step.enabled }"
      >
        <button
          type="button"
          class="row-select"
          :aria-current="step.id === selectedId ? 'true' : undefined"
          @click="selectedId = step.id"
        >
          <span class="row-index num">{{ i + 1 }}</span>
          <component
            :is="KIND_ICON[step.spec.type]"
            class="row-icon"
            :size="16"
            :stroke-width="1.75"
            :aria-label="t(`steps.kind.${step.spec.type}`)"
          />
          <span class="row-name" :title="step.name">{{ step.name || t('steps.unnamed') }}</span>
          <UiTag v-if="!step.enabled">{{ t('steps.disabled') }}</UiTag>
          <CircleAlert
            v-if="props.hasIssue(i)"
            class="row-issue"
            :size="16"
            :stroke-width="1.75"
            :aria-label="t('steps.hasIssue')"
          />
        </button>
        <UiSwitch v-model="step.enabled" :label="t('steps.enable', { name: step.name })" />
      </li>
    </ol>
    <p v-else class="empty">{{ t('steps.empty') }}</p>

    <UiConfirm
      :open="deleting"
      :title="t('steps.deleteTitle', { name: selected?.name ?? '' })"
      :confirm-label="t('common.delete')"
      danger
      @confirm="remove"
      @cancel="deleting = false"
    >
      <p class="confirm-text">{{ t('steps.deleteBody') }}</p>
    </UiConfirm>
  </section>
</template>

<style scoped>
.step-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-height: 0;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.title {
  flex: 1;
  margin: 0;
  font: var(--font-section);
}

.actions {
  display: inline-flex;
  gap: 2px;
}

.rows {
  margin: 0;
  padding: 0;
  overflow-y: auto;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-control);
  list-style: none;
}

.row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding-right: var(--space-3);
  border-bottom: 1px solid var(--border-subtle);
}

.row:last-child {
  border-bottom: 0;
}

.row.is-selected {
  background: var(--surface-selected);
  box-shadow: inset 3px 0 0 var(--action-primary);
}

.row-select {
  display: flex;
  flex: 1;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
  height: 40px;
  padding: 0 var(--space-3);
  border: 0;
  background: transparent;
  color: var(--text-primary);
  font: var(--font-body);
  text-align: left;
  cursor: pointer;
}

.row:not(.is-selected) .row-select:hover {
  background: var(--surface-hover);
}

.row.is-selected .row-name {
  font-weight: 600;
}

.row.is-disabled .row-name {
  color: var(--text-secondary);
}

.row-index {
  width: 20px;
  color: var(--text-secondary);
  text-align: right;
}

.row-icon {
  flex: none;
  color: var(--text-secondary);
}

.row-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row-issue {
  flex: none;
  color: var(--state-failure);
}

.empty {
  margin: 0;
  padding: var(--space-4);
  border: 1px dashed var(--border-subtle);
  border-radius: var(--radius-control);
  color: var(--text-secondary);
}

.confirm-text {
  margin: 0;
}
</style>

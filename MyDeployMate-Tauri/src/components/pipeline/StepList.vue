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
import { call, type Step, type StepKind } from '@/ipc'

import { moveStep, nextSelection } from './pipeline'

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
const KIND_LABEL: Record<StepKind, string> = {
  local: '本地命令',
  upload: '上传文件',
  remote: '远程命令',
  confirm: '人工确认',
}

const addEntries: MenuEntry[] = (['local', 'upload', 'remote', 'confirm'] as const).map((k) => ({
  key: k,
  label: KIND_LABEL[k],
  icon: KIND_ICON[k],
}))

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

const noSelection = computed(() => (selected.value ? null : '先选择一个步骤'))
const upReason = computed(() => noSelection.value ?? (index.value === 0 ? '已经是第一步' : null))
const downReason = computed(
  () => noSelection.value ?? (index.value === steps.value.length - 1 ? '已经是最后一步' : null),
)
</script>

<template>
  <section class="step-list" aria-label="步骤">
    <div class="toolbar">
      <h2 class="title">步骤</h2>
      <span class="actions">
        <UiIconButton :icon="ArrowUp" label="上移" :disabled-reason="upReason" @click="move(-1)" />
        <UiIconButton
          :icon="ArrowDown"
          label="下移"
          :disabled-reason="downReason"
          @click="move(1)"
        />
        <UiIconButton :icon="Copy" label="复制步骤" :disabled-reason="noSelection" @click="copy" />
        <UiIconButton
          :icon="Trash2"
          label="删除步骤"
          :disabled-reason="noSelection"
          @click="deleting = true"
        />
      </span>
      <UiMenu
        label="添加步骤"
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
            :aria-label="KIND_LABEL[step.spec.type]"
          />
          <span class="row-name" :title="step.name">{{ step.name || '（未命名）' }}</span>
          <UiTag v-if="!step.enabled">已禁用</UiTag>
          <CircleAlert
            v-if="props.hasIssue(i)"
            class="row-issue"
            :size="16"
            :stroke-width="1.75"
            aria-label="有问题需要修正"
          />
        </button>
        <UiSwitch v-model="step.enabled" :label="`启用 ${step.name}`" />
      </li>
    </ol>
    <p v-else class="empty">尚无步骤。用“添加步骤”加入本地命令、上传文件、远程命令或人工确认。</p>

    <UiConfirm
      :open="deleting"
      :title="`删除步骤 ${selected?.name ?? ''}`"
      confirm-label="删除"
      danger
      @confirm="remove"
      @cancel="deleting = false"
    >
      <p class="confirm-text">步骤及其参数会从草稿中移除，保存后生效。</p>
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

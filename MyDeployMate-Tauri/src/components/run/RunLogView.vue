<script setup lang="ts">
import { save as saveFileDialog } from '@tauri-apps/plugin-dialog'
import { ArrowDown, ChevronDown, ChevronUp, Download, WrapText } from '@lucide/vue'
import { useVirtualizer } from '@tanstack/vue-virtual'
import { computed, nextTick, onMounted, ref, watch } from 'vue'

import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { useT } from '@/i18n'
import { errorText } from '@/i18n/message'
import { call, type LogSource } from '@/ipc'
import type { RunDetail } from '@/stores/run'
import { useToastStore } from '@/stores/toast'

import { clockTime, highlight, LIVE_LINES, type LogLine } from './runLog'
import { exportFileName, formatRunLog } from './runLogExport'

const t = useT()

// IXD-27 / UI-26: the run log. Follows new output until the user scrolls up; each task filter
// keeps its own reading position. Text is always rendered as plain text, never as HTML.
const props = defineProps<{ detail: RunDetail; selected: number | null }>()

type TaskFilter = 'all' | number
const taskFilter = ref<TaskFilter>(props.selected ?? 'all')
const serverFilter = ref<string>('all')
const sourceFilter = ref<'all' | LogSource>('all')
const failedOnly = ref(false)
const query = ref('')
const wrap = ref(false)

watch(
  () => props.selected,
  (seq) => {
    if (seq !== null) taskFilter.value = seq
  },
)

const taskOptions = computed(() => [
  { value: 'all' as TaskFilter, label: t('log.allTasks') },
  ...props.detail.tasks.map((t) => ({
    value: t.seq as TaskFilter,
    label: `${t.seq + 1} · ${t.stepName}${t.serverName ? ` → ${t.serverName}` : ''}`,
  })),
])
const servers = computed(() => props.detail.plan.targets.filter((t) => t.selected))
const serverOptions = computed(() => [
  { value: 'all', label: t('log.allServers') },
  { value: 'local', label: t('common.local') },
  ...servers.value.map((s) => ({ value: s.serverId, label: s.name })),
])
const sourceLabel = (source: LogSource) => (source === 'system' ? t('log.system') : source)
const sourceOptions = computed(() => [
  { value: 'all' as const, label: t('log.allSources') },
  { value: 'stdout' as const, label: 'stdout' },
  { value: 'stderr' as const, label: 'stderr' },
  { value: 'system' as const, label: t('log.system') },
])

const filtering = computed(
  () =>
    taskFilter.value !== 'all' ||
    serverFilter.value !== 'all' ||
    sourceFilter.value !== 'all' ||
    failedOnly.value,
)

const total = computed(() => {
  void props.detail.logRevision
  return props.detail.lines.lines.length
})

const trimmed = computed(() => {
  void props.detail.logRevision
  return props.detail.lines.trimmed
})

const lines = computed<LogLine[]>(() => {
  void props.detail.logRevision
  const all = props.detail.lines.lines
  if (!filtering.value) return all.slice()
  const serverOf = (seq: number) => props.detail.plan.tasks[seq]?.server?.id ?? 'local'
  const failed = new Set(
    props.detail.tasks
      .filter((t) => t.status === 'failed' || t.status === 'unknown')
      .map((t) => t.seq),
  )
  return all.filter(
    (l) =>
      (taskFilter.value === 'all' || l.task === taskFilter.value) &&
      (serverFilter.value === 'all' || serverOf(l.task) === serverFilter.value) &&
      (sourceFilter.value === 'all' || l.source === sourceFilter.value) &&
      (!failedOnly.value || failed.has(l.task)),
  )
})

const scroller = ref<HTMLElement | null>(null)
const virtualizer = useVirtualizer(
  computed(() => ({
    count: lines.value.length,
    getScrollElement: () => scroller.value,
    estimateSize: () => 20,
    overscan: 30,
    getItemKey: (i: number) => lines.value[i]?.index ?? i,
  })),
)
const rows = computed(() => virtualizer.value.getVirtualItems())
function measure(el: unknown) {
  if (el instanceof Element) virtualizer.value.measureElement(el)
}
watch(wrap, () => nextTick(() => virtualizer.value.measure()))

// Following: on until the user scrolls away from the bottom; back on at the bottom.
const following = ref(true)
const seenCount = ref(0)
let programmatic = false

function toBottom() {
  const el = scroller.value
  if (!el) return
  // Set the offset directly: the virtualizer's own "end" alignment measures the box with its
  // horizontal scrollbar and stops a line or two short of the last one.
  programmatic = true
  el.scrollTop = el.scrollHeight
  requestAnimationFrame(() => {
    el.scrollTop = el.scrollHeight
    requestAnimationFrame(() => (programmatic = false))
  })
}

function onScroll() {
  const el = scroller.value
  if (!el || programmatic) return
  const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 8
  if (atBottom !== following.value) {
    following.value = atBottom
    seenCount.value = lines.value.length
  }
}

const newLines = computed(() =>
  following.value ? 0 : Math.max(0, lines.value.length - seenCount.value),
)

function follow() {
  following.value = true
  void nextTick(toBottom)
}

watch(
  () => [props.detail.logRevision, lines.value.length],
  () => {
    if (following.value) void nextTick(toBottom)
  },
)
onMounted(() => void nextTick(toBottom))

// IXD-27: switching tasks keeps each one's reading position.
const positions = new Map<TaskFilter, { top: number; following: boolean; seen: number }>()
watch(taskFilter, (next, prev) => {
  const el = scroller.value
  if (el)
    positions.set(prev, { top: el.scrollTop, following: following.value, seen: seenCount.value })
  const saved = positions.get(next)
  following.value = saved?.following ?? true
  seenCount.value = saved?.seen ?? 0
  void nextTick(() => {
    if (following.value || !saved) return toBottom()
    programmatic = true
    virtualizer.value.scrollToOffset(saved.top)
    requestAnimationFrame(() => (programmatic = false))
  })
})

// Search: highlights only; the text itself is never changed.
const needle = computed(() => query.value.trim())
const matches = computed(() => {
  const q = needle.value.toLowerCase()
  if (!q) return []
  const out: number[] = []
  lines.value.forEach((l, i) => {
    if (l.text.toLowerCase().includes(q)) out.push(i)
  })
  return out
})
const current = ref(0)
watch(needle, () => (current.value = 0))

function step(by: number) {
  const n = matches.value.length
  if (!n) return
  current.value = (current.value + by + n) % n
  following.value = false
  seenCount.value = lines.value.length
  programmatic = true
  virtualizer.value.scrollToIndex(matches.value[current.value] ?? 0, { align: 'center' })
  requestAnimationFrame(() => (programmatic = false))
}
const currentLine = computed(() =>
  matches.value.length ? (matches.value[current.value] ?? -1) : -1,
)

function onSearchKey(event: KeyboardEvent) {
  if (event.key !== 'Enter') return
  event.preventDefault()
  step(event.shiftKey ? -1 : 1)
}

const showTask = computed(() => taskFilter.value === 'all')

// IXD-27 / OBS-02: export asks first, with the reminder that redaction is not complete.
const toast = useToastStore()
const askExport = ref(false)
const exporting = ref(false)
async function exportLog() {
  exporting.value = true
  try {
    const path = await saveFileDialog({
      defaultPath: exportFileName(props.detail),
      filters: [{ name: 'Log', extensions: ['log', 'txt'] }],
    })
    if (!path) return
    await call('write_export', { path, contents: formatRunLog(props.detail) })
    askExport.value = false
    toast.show(t('log.export.done'))
  } catch (e) {
    toast.show(errorText(e))
  } finally {
    exporting.value = false
  }
}
</script>

<template>
  <section class="log" :aria-label="t('log.label')">
    <div class="toolbar">
      <UiSelect
        v-model="taskFilter"
        class="filter filter--task"
        :options="taskOptions"
        compact
        :aria-label="t('log.task')"
      />
      <UiSelect
        v-if="servers.length"
        v-model="serverFilter"
        class="filter"
        :options="serverOptions"
        compact
        :aria-label="t('common.server')"
      />
      <UiSelect
        v-model="sourceFilter"
        class="filter"
        :options="sourceOptions"
        compact
        :aria-label="t('log.source')"
      />
      <span class="toggle">
        <UiSwitch id="log-failed" v-model="failedOnly" />
        <label for="log-failed">{{ t('log.failedOnly') }}</label>
      </span>
      <div class="search" @keydown="onSearchKey">
        <UiTextInput v-model="query" :placeholder="t('log.search')" :aria-label="t('log.search')" />
        <span v-if="needle" class="search-count num">
          {{ matches.length ? `${current + 1}/${matches.length}` : '0/0' }}
        </span>
        <UiIconButton
          :icon="ChevronUp"
          :label="t('log.prev')"
          :disabled-reason="matches.length ? null : t('log.noMatch')"
          @click="step(-1)"
        />
        <UiIconButton
          :icon="ChevronDown"
          :label="t('log.next')"
          :disabled-reason="matches.length ? null : t('log.noMatch')"
          @click="step(1)"
        />
      </div>
      <UiIconButton :icon="WrapText" :label="t('log.wrap')" :pressed="wrap" @click="wrap = !wrap" />
      <UiIconButton
        :icon="Download"
        :label="t('log.exportButton')"
        :disabled-reason="detail.live ? t('log.exportLive') : null"
        @click="askExport = true"
      />
    </div>
    <p class="count">
      <i18n-t :keypath="filtering ? 'log.countFiltered' : 'log.count'">
        <template #shown>
          <span class="num">{{ lines.length }}</span>
        </template>
        <template #total>
          <span class="num">{{ total }}</span>
        </template>
      </i18n-t>
      <template v-if="trimmed">
        ·
        <i18n-t keypath="log.trimmed">
          <template #n>
            <span class="num">{{ LIVE_LINES }}</span>
          </template>
        </i18n-t>
      </template>
      <template v-if="detail.damagedLines">
        · {{ t('log.damaged', { n: detail.damagedLines }) }}
      </template>
    </p>
    <div class="body">
      <div ref="scroller" class="scroller" :class="{ wrap }" tabindex="0" @scroll="onScroll">
        <p v-if="!lines.length" class="empty">
          {{ total ? t('log.noMatches') : t('log.noOutput') }}
        </p>
        <div class="space" :style="{ height: `${virtualizer.getTotalSize()}px` }">
          <div
            v-for="row in rows"
            :key="String(row.key)"
            :ref="measure"
            class="row"
            :class="[
              `row--${lines[row.index]?.source}`,
              {
                'row--hit': row.index === currentLine,
                'row--marker': lines[row.index]?.truncated,
              },
            ]"
            :data-index="row.index"
            :style="{ transform: `translateY(${row.start}px)` }"
          >
            <span class="time num">{{ clockTime(lines[row.index]?.atMs ?? 0) }}</span>
            <span class="source">{{ sourceLabel(lines[row.index]?.source ?? 'stdout') }}</span>
            <span v-if="showTask" class="task num">#{{ (lines[row.index]?.task ?? 0) + 1 }}</span>
            <span class="text"
              ><template
                v-for="(part, i) in highlight(lines[row.index]?.text ?? '', needle)"
                :key="i"
                ><mark v-if="part.hit">{{ part.text }}</mark
                ><template v-else>{{ part.text }}</template></template
              ></span
            >
          </div>
        </div>
      </div>
      <button v-if="!following" type="button" class="latest" @click="follow">
        <ArrowDown :size="16" :stroke-width="1.75" aria-hidden="true" />
        <span>{{ t('log.latest') }}</span>
        <span v-if="newLines" class="num">{{ t('log.newLines', { n: newLines }) }}</span>
      </button>
    </div>
    <UiConfirm
      :open="askExport"
      :title="t('log.export.title')"
      :confirm-label="t('log.export.confirm')"
      :busy="exporting"
      @cancel="askExport = false"
      @confirm="exportLog"
    >
      <p>{{ t('log.export.body') }}</p>
      <p class="export-warning">{{ t('log.export.sensitive') }}</p>
    </UiConfirm>
  </section>
</template>

<style scoped>
.export-warning {
  margin-top: var(--space-2);
  color: var(--state-warning);
}

.log {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1;
}

.toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
}

.filter {
  width: 128px;
}

.filter--task {
  width: 200px;
}

.toggle {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  font: var(--font-caption);
}

.search {
  display: flex;
  flex: 1;
  align-items: center;
  gap: var(--space-1);
  min-width: 200px;
}

.search :deep(.text-input) {
  flex: 1;
}

.search-count {
  color: var(--text-secondary);
  font: var(--font-caption);
}

.count {
  margin: var(--space-1) 0;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.body {
  position: relative;
  flex: 1;
  min-height: 160px;
}

.scroller {
  position: absolute;
  inset: 0;
  overflow: auto;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-control);
  background: var(--surface-canvas);
  font: var(--font-code);
}

.space {
  position: relative;
  min-width: 100%;
}

.row {
  position: absolute;
  top: 0;
  left: 0;
  display: flex;
  min-width: 100%;
  padding: 0 var(--space-2);
  white-space: pre;
}

.wrap .row {
  right: 0;
  white-space: pre-wrap;
}

.wrap .text {
  min-width: 0;
  overflow-wrap: anywhere;
}

.time {
  flex: none;
  width: 108px;
  color: var(--text-secondary);
}

.source {
  flex: none;
  width: 64px;
  color: var(--text-secondary);
}

.task {
  flex: none;
  width: 40px;
  color: var(--text-secondary);
}

.text {
  flex: 1;
}

.row--stderr {
  box-shadow: inset 2px 0 0 var(--state-failure);
}

.row--stderr .source {
  color: var(--state-failure);
}

.row--system .text {
  color: var(--text-secondary);
}

.row--marker .text {
  color: var(--state-warning);
}

.row--hit {
  background: var(--surface-selected);
}

mark {
  background: var(--state-warning-bg);
  color: inherit;
  outline: 1px solid var(--state-warning);
}

.empty {
  position: absolute;
  margin: var(--space-3);
  color: var(--text-secondary);
  font: var(--font-body);
}

.latest {
  position: absolute;
  right: var(--space-4);
  bottom: var(--space-3);
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  height: var(--button-height-compact);
  padding: 0 var(--space-3);
  border: 1px solid var(--border-control);
  border-radius: var(--radius-control);
  background: var(--surface-panel);
  box-shadow: var(--shadow-toast);
  color: var(--text-primary);
  font: var(--font-label);
  cursor: pointer;
}
</style>

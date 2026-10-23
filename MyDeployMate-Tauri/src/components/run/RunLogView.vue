<script setup lang="ts">
import { ArrowDown, ChevronDown, ChevronUp, WrapText } from '@lucide/vue'
import { useVirtualizer } from '@tanstack/vue-virtual'
import { computed, nextTick, onMounted, ref, watch } from 'vue'

import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import type { LogSource } from '@/ipc'
import type { RunDetail } from '@/stores/run'

import { clockTime, highlight, type LogLine } from './runLog'

// IXD-27 / UI-26: the run log. Follows new output until the user scrolls up; each task filter
// keeps its own reading position. Text is always rendered as plain text, never as HTML.
const props = defineProps<{ detail: RunDetail; selected: number | null }>()

const SOURCE: Record<LogSource, string> = { stdout: 'stdout', stderr: 'stderr', system: '系统' }

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
  { value: 'all' as TaskFilter, label: '全部任务' },
  ...props.detail.tasks.map((t) => ({
    value: t.seq as TaskFilter,
    label: `${t.seq + 1} · ${t.stepName}${t.serverName ? ` → ${t.serverName}` : ''}`,
  })),
])
const servers = computed(() => props.detail.plan.targets.filter((t) => t.selected))
const serverOptions = computed(() => [
  { value: 'all', label: '全部服务器' },
  { value: 'local', label: '本地' },
  ...servers.value.map((s) => ({ value: s.serverId, label: s.name })),
])
const sourceOptions = [
  { value: 'all' as const, label: '全部来源' },
  { value: 'stdout' as const, label: 'stdout' },
  { value: 'stderr' as const, label: 'stderr' },
  { value: 'system' as const, label: '系统' },
]

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
  programmatic = true
  virtualizer.value.scrollToIndex(lines.value.length - 1, { align: 'end' })
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
</script>

<template>
  <section class="log" aria-label="日志">
    <div class="toolbar">
      <UiSelect
        v-model="taskFilter"
        class="filter filter--task"
        :options="taskOptions"
        compact
        aria-label="任务"
      />
      <UiSelect
        v-if="servers.length"
        v-model="serverFilter"
        class="filter"
        :options="serverOptions"
        compact
        aria-label="服务器"
      />
      <UiSelect
        v-model="sourceFilter"
        class="filter"
        :options="sourceOptions"
        compact
        aria-label="来源"
      />
      <span class="toggle">
        <UiSwitch id="log-failed" v-model="failedOnly" />
        <label for="log-failed">仅失败任务</label>
      </span>
      <div class="search" @keydown="onSearchKey">
        <UiTextInput v-model="query" placeholder="搜索日志" aria-label="搜索日志" />
        <span v-if="needle" class="search-count num">
          {{ matches.length ? `${current + 1}/${matches.length}` : '0/0' }}
        </span>
        <UiIconButton
          :icon="ChevronUp"
          label="上一个"
          :disabled-reason="matches.length ? null : '没有匹配'"
          @click="step(-1)"
        />
        <UiIconButton
          :icon="ChevronDown"
          label="下一个"
          :disabled-reason="matches.length ? null : '没有匹配'"
          @click="step(1)"
        />
      </div>
      <UiIconButton :icon="WrapText" label="自动换行" :pressed="wrap" @click="wrap = !wrap" />
    </div>
    <p class="count">
      <template v-if="filtering">
        显示 <span class="num">{{ lines.length }}</span> / <span class="num">{{ total }}</span> 行 ·
        已筛选
      </template>
      <template v-else
        >共 <span class="num">{{ total }}</span> 行</template
      >
      <template v-if="detail.damagedLines">
        · {{ detail.damagedLines }} 行日志损坏，未显示
      </template>
    </p>
    <div class="body">
      <div ref="scroller" class="scroller" :class="{ wrap }" tabindex="0" @scroll="onScroll">
        <p v-if="!lines.length" class="empty">
          {{ total ? '没有符合筛选条件的日志' : '还没有输出' }}
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
            <span class="source">{{ SOURCE[lines[row.index]?.source ?? 'stdout'] }}</span>
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
        <span>回到最新</span>
        <span v-if="newLines" class="num">（{{ newLines }} 行新输出）</span>
      </button>
    </div>
  </section>
</template>

<style scoped>
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
  width: 56px;
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

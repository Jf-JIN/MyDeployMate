<script setup lang="ts">
import { History, SearchX, Trash2 } from '@lucide/vue'
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import EmptyState from '@/components/EmptyState.vue'
import PageLayout from '@/components/PageLayout.vue'
import RunStatusTag from '@/components/run/RunStatusTag.vue'
import { dateTime, duration, runFinished } from '@/components/run/runState'
import UiButton from '@/components/ui/UiButton.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiField from '@/components/ui/UiField.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import type { SelectOption } from '@/components/ui/types'
import { useT } from '@/i18n'
import { errorText } from '@/i18n/message'
import { call, type RunChoice, type RunFilter, type RunSummary, type StatusFilter } from '@/ipc'
import { useRunStore } from '@/stores/run'
import { useToastStore } from '@/stores/toast'
import { useWorkspaceStore } from '@/stores/workspace'

const t = useT()

// P08 run history (IXD-34): filters by project, pipeline, environment, result and time; a row
// opens the read-only P07; finished runs can be deleted after a confirmation (OBS-02).
const store = useWorkspaceStore()
const runs = useRunStore()
const toast = useToastStore()
const router = useRouter()

const rows = ref<RunSummary[]>([])
// NFR-01: a long history renders in pages, so the page opens within 200 ms at 1000 runs.
const PAGE = 200
const shown = ref(PAGE)
const visibleRows = computed(() => rows.value.slice(0, shown.value))
const choices = ref<RunChoice[]>([])
const loaded = ref(false)
const loadError = ref<string | null>(null)

type Range = '' | 'day' | 'week' | 'month'
const RANGE_MS: Record<Exclude<Range, ''>, number> = {
  day: 24 * 3600_000,
  week: 7 * 24 * 3600_000,
  month: 30 * 24 * 3600_000,
}

const projectId = ref('')
const pipelineId = ref('')
const environmentId = ref('')
const status = ref<StatusFilter | ''>('')
const range = ref<Range>('')

const filtered = computed(
  () =>
    !!(projectId.value || pipelineId.value || environmentId.value || status.value || range.value),
)

function filter(): RunFilter {
  return {
    projectId: projectId.value || null,
    pipelineId: pipelineId.value || null,
    environmentId: environmentId.value || null,
    status: status.value || null,
    sinceMs: range.value ? Date.now() - RANGE_MS[range.value] : null,
    untilMs: null,
  }
}

async function load() {
  try {
    const [list, names] = await Promise.all([
      call('list_runs', { filter: filter() }),
      call('run_choices'),
    ])
    rows.value = list
    shown.value = PAGE
    choices.value = names
    loadError.value = null
  } catch (e) {
    loadError.value = errorText(e)
  } finally {
    loaded.value = true
  }
}

onMounted(load)
watch([projectId, pipelineId, environmentId, status, range], load)
// A run that starts or ends while the page is open changes the list.
watch(() => runs.active?.runId ?? null, load)

// A pipeline picked under another project no longer applies.
watch(projectId, () => {
  if (pipelineId.value && !pipelineOptions.value.some((o) => o.value === pipelineId.value)) {
    pipelineId.value = ''
  }
})

function clearFilters() {
  projectId.value = ''
  pipelineId.value = ''
  environmentId.value = ''
  status.value = ''
  range.value = ''
}

// ---- Filter options: current names, or the snapshot name of a deleted object ---------------

function uniqueBy<K>(list: RunChoice[], key: (c: RunChoice) => K): RunChoice[] {
  const seen = new Set<K>()
  return list.filter((c) => !seen.has(key(c)) && !!seen.add(key(c)))
}

const deleted = (name: string) => t('history.filters.deleted', { name })

function projectLabel(c: RunChoice) {
  return store.projects.find((p) => p.id === c.projectId)?.name ?? deleted(c.projectName)
}

function pipelineLabel(c: RunChoice) {
  const project = store.projects.find((p) => p.id === c.projectId)
  return project?.pipelines.find((p) => p.id === c.pipelineId)?.name ?? deleted(c.pipelineName)
}

function environmentLabel(c: RunChoice) {
  return (
    store.environments.find((e) => e.id === c.environmentId)?.name ?? deleted(c.environmentName)
  )
}

const all = computed(() => t('history.filters.all'))

const projectOptions = computed<SelectOption<string>[]>(() => [
  { value: '', label: all.value },
  ...uniqueBy(choices.value, (c) => c.projectId).map((c) => ({
    value: c.projectId,
    label: projectLabel(c),
  })),
])

const pipelineOptions = computed<SelectOption<string>[]>(() => [
  { value: '', label: all.value },
  ...uniqueBy(
    choices.value.filter((c) => !projectId.value || c.projectId === projectId.value),
    (c) => c.pipelineId,
  ).map((c) => ({
    value: c.pipelineId,
    label: projectId.value ? pipelineLabel(c) : `${projectLabel(c)} / ${pipelineLabel(c)}`,
  })),
])

const environmentOptions = computed<SelectOption<string>[]>(() => [
  { value: '', label: all.value },
  ...uniqueBy(choices.value, (c) => c.environmentId).map((c) => ({
    value: c.environmentId,
    label: environmentLabel(c),
  })),
])

const STATUSES: StatusFilter[] = ['active', 'succeeded', 'failed', 'cancelled', 'interrupted']
const statusOptions = computed<SelectOption<StatusFilter | ''>[]>(() => [
  { value: '', label: all.value },
  ...STATUSES.map((s) => ({ value: s, label: t(`history.filters.statuses.${s}`) })),
])

const rangeOptions = computed<SelectOption<Range>[]>(() => [
  { value: '', label: all.value },
  { value: 'day', label: t('history.filters.ranges.day') },
  { value: 'week', label: t('history.filters.ranges.week') },
  { value: 'month', label: t('history.filters.ranges.month') },
])

// ---- Rows ----------------------------------------------------------------------------------

function open(row: RunSummary) {
  void router.push({ name: 'run', params: { runId: row.run.id } })
}

function openFromRow(event: MouseEvent, row: RunSummary) {
  if ((event.target as Element).closest('a, button')) return
  open(row)
}

function runDuration(row: RunSummary) {
  const { createdAtMs, finishedAtMs } = row.run
  return finishedAtMs === null ? null : duration(finishedAtMs - createdAtMs)
}

// ---- Delete --------------------------------------------------------------------------------

const deleting = ref<RunSummary | null>(null)
const deleteBusy = ref(false)

function deleteReason(row: RunSummary) {
  return runFinished(row.run.status) ? null : t('history.deleteBlocked')
}

async function applyDelete() {
  const row = deleting.value
  if (!row) return
  deleteBusy.value = true
  try {
    await call('delete_run', { runId: row.run.id })
    toast.show(t('history.deleted'))
  } catch (e) {
    loadError.value = errorText(e)
  } finally {
    deleteBusy.value = false
    deleting.value = null
  }
  await load()
}
</script>

<template>
  <PageLayout :title="t('history.title')">
    <div class="history">
      <UiNotice v-if="loadError" tone="failure" dismissible @dismiss="loadError = null">
        {{ loadError }}
      </UiNotice>

      <EmptyState
        v-if="loaded && !choices.length"
        :title="t('history.emptyTitle')"
        :detail="t('history.emptyDetail')"
        :icon="History"
      />

      <template v-else-if="loaded">
        <div class="filters" role="search" :aria-label="t('history.filters.label')">
          <UiField v-slot="{ id }" :label="t('history.filters.project')">
            <UiSelect :id="id" v-model="projectId" :options="projectOptions" compact />
          </UiField>
          <UiField v-slot="{ id }" :label="t('history.filters.pipeline')">
            <UiSelect :id="id" v-model="pipelineId" :options="pipelineOptions" compact />
          </UiField>
          <UiField v-slot="{ id }" :label="t('history.filters.environment')">
            <UiSelect :id="id" v-model="environmentId" :options="environmentOptions" compact />
          </UiField>
          <UiField v-slot="{ id }" :label="t('history.filters.status')">
            <UiSelect :id="id" v-model="status" :options="statusOptions" compact />
          </UiField>
          <UiField v-slot="{ id }" :label="t('history.filters.range')">
            <UiSelect :id="id" v-model="range" :options="rangeOptions" compact />
          </UiField>
          <span class="count num">{{ t('history.count', { n: rows.length }) }}</span>
        </div>

        <div class="table-wrap">
          <table v-if="rows.length" class="table">
            <thead>
              <tr>
                <th scope="col">{{ t('history.columns.started') }}</th>
                <th scope="col">{{ t('history.columns.pipeline') }}</th>
                <th scope="col">{{ t('history.columns.environment') }}</th>
                <th scope="col">{{ t('history.columns.version') }}</th>
                <th scope="col" class="num">{{ t('history.columns.targets') }}</th>
                <th scope="col">{{ t('history.columns.result') }}</th>
                <th scope="col" class="num">{{ t('history.columns.duration') }}</th>
                <th scope="col" class="col-actions">
                  <span class="sr-only">{{ t('common.actions') }}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="row in visibleRows"
                :key="row.run.id"
                class="row"
                @click="openFromRow($event, row)"
              >
                <td class="nowrap tabular">
                  <RouterLink
                    class="name"
                    :to="{ name: 'run', params: { runId: row.run.id } }"
                    :aria-label="
                      t('history.open', {
                        project: row.run.projectName,
                        pipeline: row.run.pipelineName,
                        time: dateTime(row.run.createdAtMs),
                      })
                    "
                  >
                    {{ dateTime(row.run.createdAtMs) }}
                  </RouterLink>
                </td>
                <td>{{ row.run.projectName }} / {{ row.run.pipelineName }}</td>
                <td>{{ row.run.environmentName }}</td>
                <td class="num">{{ row.run.version }}</td>
                <td class="num">
                  <template v-if="row.run.localOnly">{{ t('history.localOnly') }}</template>
                  <template v-else>{{ row.targetCount }}</template>
                </td>
                <td><RunStatusTag :status="row.run.status" /></td>
                <td class="num nowrap">{{ runDuration(row) ?? '—' }}</td>
                <td class="col-actions">
                  <UiIconButton
                    :icon="Trash2"
                    :label="t('history.deleteItem', { time: dateTime(row.run.createdAtMs) })"
                    :disabled-reason="deleteReason(row)"
                    @click="deleting = row"
                  />
                </td>
              </tr>
            </tbody>
          </table>
          <div v-if="rows.length > shown" class="more">
            <UiButton compact @click="shown += PAGE">
              {{ t('history.showMore', { n: rows.length - shown }) }}
            </UiButton>
          </div>
          <EmptyState v-if="!rows.length" :title="t('history.noMatch')" :icon="SearchX" compact>
            <UiButton v-if="filtered" compact @click="clearFilters">
              {{ t('history.filters.clear') }}
            </UiButton>
          </EmptyState>
        </div>
      </template>
    </div>

    <UiConfirm
      :open="!!deleting"
      :title="t('history.delete.title')"
      :confirm-label="t('history.delete.confirm')"
      danger
      :busy="deleteBusy"
      @cancel="deleting = null"
      @confirm="applyDelete"
    >
      <p v-if="deleting">
        {{
          t('history.delete.body', {
            project: deleting.run.projectName,
            pipeline: deleting.run.pipelineName,
            time: dateTime(deleting.run.createdAtMs),
          })
        }}
      </p>
    </UiConfirm>
  </PageLayout>
</template>

<style scoped>
.history {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.filters {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--space-3);
}

.filters :deep(.field) {
  width: 180px;
}

.count {
  margin-left: auto;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.table-wrap {
  overflow-x: auto;
}

.more {
  display: flex;
  justify-content: center;
  padding: var(--space-3) 0;
}

.table {
  min-width: 900px;
}

.row {
  cursor: pointer;
}

.row:hover > td {
  background: var(--surface-hover);
}

.name {
  color: var(--text-primary);
  font-weight: 500;
  text-decoration: none;
}

.row:hover .name {
  color: var(--action-primary);
}

.nowrap {
  white-space: nowrap;
}

.tabular {
  font-variant-numeric: tabular-nums;
}

.col-actions {
  width: 56px;
}
</style>

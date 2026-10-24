<script setup lang="ts">
import { ArrowDown, ArrowUp, Search, SearchX, ServerOff, Shield } from '@lucide/vue'
import { computed, ref } from 'vue'

import DraftNotices from '@/components/DraftNotices.vue'
import EmptyState from '@/components/EmptyState.vue'
import { moveTarget, setTargetIds, targetIds } from '@/components/projects/targets'
import { hostPort } from '@/components/servers/serverForm'
import { testStatus } from '@/components/servers/testStatus'
import UiButton from '@/components/ui/UiButton.vue'
import UiCheckbox from '@/components/ui/UiCheckbox.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import UiTag from '@/components/ui/UiTag.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { useProjectDraft } from '@/composables/projectDraft'
import { useT } from '@/i18n'
import type { Pipeline, Project, Server } from '@/ipc'
import { useConnectionTestStore } from '@/stores/connectionTests'
import { useSessionStore } from '@/stores/session'
import { useWorkspaceStore } from '@/stores/workspace'

const t = useT()

// P03 target binding (IXD-12): servers of the current environment, checked ones in execution
// order. Checking is a draft until saved; switching environment goes through the leave guard.
const props = defineProps<{ project: Project; pipeline: Pipeline | null }>()

const store = useWorkspaceStore()
const session = useSessionStore()
const tests = useConnectionTestStore()

function statusOf(server: Server) {
  return testStatus(tests.results[server.id], !!tests.running[server.id])
}

const page = ref<HTMLElement | null>(null)
const { project, fieldError, outside, error } = useProjectDraft(
  'drafts.labels.targets',
  page,
  () => props.project.id,
)
const elsewhere = outside(['targets'])

const environmentId = computed(
  () => session.environment[props.project.id] ?? props.project.defaultEnvironmentId,
)
const environment = computed(() => store.environments.find((e) => e.id === environmentId.value))

const selectedIds = computed(() =>
  project.value ? targetIds(project.value, environmentId.value) : [],
)
const selected = computed(() => new Set(selectedIds.value))

function setSelected(ids: string[]) {
  if (project.value) setTargetIds(project.value, environmentId.value, ids)
}

const query = ref('')
const onlySelected = ref(false)

function clearFilters() {
  query.value = ''
  onlySelected.value = false
}

/** Checked servers first in execution order, then the rest by name. */
const rows = computed(() => {
  const inEnv = store.servers.filter((s) => s.environmentId === environmentId.value)
  const byId = new Map(inEnv.map((s) => [s.id, s]))
  const chosen = selectedIds.value.map((id) => byId.get(id)).filter((s): s is Server => !!s)
  const rest = inEnv
    .filter((s) => !selected.value.has(s.id))
    .sort((a, b) => a.name.localeCompare(b.name, 'zh-CN', { numeric: true }))
  return [...chosen, ...rest]
})

const serverCount = computed(() => rows.value.length)

const visible = computed(() => {
  const q = query.value.trim().toLowerCase()
  return rows.value.filter((s) => {
    if (onlySelected.value && !selected.value.has(s.id)) return false
    if (!q) return true
    return [s.name, s.host, s.user, ...s.tags].some((t) => t.toLowerCase().includes(q))
  })
})

function toggle(id: string, on: boolean) {
  const ids = selectedIds.value.filter((x) => x !== id)
  setSelected(on ? [...ids, id] : ids)
}

// IXD-12: the header checkbox affects only what the filter shows.
const visibleSelected = computed(() => visible.value.filter((s) => selected.value.has(s.id)))
const allVisibleSelected = computed(
  () => visible.value.length > 0 && visibleSelected.value.length === visible.value.length,
)
function toggleVisible(on: boolean) {
  const shown = new Set(visible.value.map((s) => s.id))
  if (on) {
    const added = visible.value.filter((s) => !selected.value.has(s.id)).map((s) => s.id)
    setSelected([...selectedIds.value, ...added])
  } else {
    setSelected(selectedIds.value.filter((id) => !shown.has(id)))
  }
}

function order(id: string) {
  return selectedIds.value.indexOf(id) + 1
}

function move(id: string, delta: -1 | 1) {
  setSelected(moveTarget(selectedIds.value, id, delta))
}

// Bindings that point at a server no longer in this environment (validation reports these).
const bindingErrors = computed(() =>
  selectedIds.value
    .map((_, i) => fieldError(`targets[${targetIndex.value}].serverIds[${i}]`))
    .filter((m): m is string => !!m),
)
const targetIndex = computed(
  () => project.value?.targets.findIndex((t) => t.environmentId === environmentId.value) ?? -1,
)
</script>

<template>
  <div v-if="project" ref="page" class="targets">
    <DraftNotices :error="error" :issues="elsewhere" />

    <header class="summary">
      <span class="env">
        <i18n-t keypath="targets.currentEnv">
          <template #environment>
            <strong>{{ environment?.name }}</strong>
          </template>
        </i18n-t>
        <UiTag v-if="environment?.protected" tone="warning" :icon="Shield">
          {{ t('common.protectedEnv') }}
        </UiTag>
      </span>
      <i18n-t keypath="targets.selected" tag="span" class="count">
        <template #count>
          <strong class="num">{{ selectedIds.length }}</strong>
        </template>
      </i18n-t>
      <p class="hint">{{ t('targets.hint') }}</p>
    </header>

    <ul v-if="bindingErrors.length" class="binding-errors">
      <li v-for="m in bindingErrors" :key="m">{{ m }}</li>
    </ul>

    <template v-if="serverCount">
      <div class="filters" role="search">
        <div class="search">
          <Search class="search-icon" :size="16" :stroke-width="1.75" aria-hidden="true" />
          <UiTextInput
            v-model="query"
            :placeholder="t('targets.search')"
            :aria-label="t('servers.searchLabel')"
          />
        </div>
        <span class="only-selected">
          <UiSwitch id="only-selected" v-model="onlySelected" />
          <label for="only-selected">{{ t('targets.onlySelected') }}</label>
        </span>
      </div>

      <div class="table-wrap">
        <table v-if="visible.length" class="table">
          <thead>
            <tr>
              <th scope="col" class="col-check">
                <UiCheckbox
                  :model-value="allVisibleSelected"
                  :indeterminate="!allVisibleSelected && visibleSelected.length > 0"
                  :label="t('targets.selectVisible')"
                  hide-label
                  @update:model-value="toggleVisible"
                />
              </th>
              <th scope="col" class="col-order num">{{ t('targets.order') }}</th>
              <th scope="col" class="col-name">{{ t('common.name') }}</th>
              <th scope="col">{{ t('common.address') }}</th>
              <th scope="col">{{ t('targets.account') }}</th>
              <th scope="col">{{ t('servers.tags') }}</th>
              <th scope="col">{{ t('servers.lastTest') }}</th>
              <th scope="col" class="col-move">
                <span class="sr-only">{{ t('targets.reorder') }}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="server in visible" :key="server.id" :data-server-id="server.id">
              <td class="col-check">
                <UiCheckbox
                  :model-value="selected.has(server.id)"
                  :label="t('servers.select', { name: server.name })"
                  hide-label
                  @update:model-value="toggle(server.id, $event)"
                />
              </td>
              <td class="col-order num">{{ selected.has(server.id) ? order(server.id) : '' }}</td>
              <td class="col-name">{{ server.name }}</td>
              <td>
                <code>{{ hostPort(server.host, server.port) }}</code>
              </td>
              <td>{{ server.user }}</td>
              <td>
                <span class="tags">
                  <UiTag v-for="tag in server.tags" :key="tag">{{ tag }}</UiTag>
                </span>
              </td>
              <td>
                <UiTag
                  :tone="statusOf(server).tone"
                  :icon="statusOf(server).icon"
                  :spin="statusOf(server).spin"
                >
                  {{ statusOf(server).label }}
                </UiTag>
              </td>
              <td class="col-move">
                <span v-if="selected.has(server.id)" class="move">
                  <UiIconButton
                    :icon="ArrowUp"
                    :label="t('targets.moveUp', { name: server.name })"
                    :disabled-reason="order(server.id) === 1 ? t('targets.first') : null"
                    @click="move(server.id, -1)"
                  />
                  <UiIconButton
                    :icon="ArrowDown"
                    :label="t('targets.moveDown', { name: server.name })"
                    :disabled-reason="
                      order(server.id) === selectedIds.length ? t('targets.last') : null
                    "
                    @click="move(server.id, 1)"
                  />
                </span>
              </td>
            </tr>
          </tbody>
        </table>
        <EmptyState v-else :title="t('servers.noMatch')" :icon="SearchX" compact>
          <UiButton compact @click="clearFilters">{{ t('common.clearFilters') }}</UiButton>
        </EmptyState>
      </div>
    </template>

    <EmptyState
      v-else
      :title="t('targets.emptyTitle')"
      :detail="t('targets.emptyDetail')"
      :icon="ServerOff"
    >
      <RouterLink :to="{ name: 'servers' }" class="empty-link">
        {{ t('targets.manageServers') }}
      </RouterLink>
    </EmptyState>
  </div>
</template>

<style scoped>
.targets {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.summary {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2) var(--space-5);
}

.env {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
}

.hint {
  flex-basis: 100%;
  margin: 0;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.binding-errors {
  margin: 0;
  padding-left: var(--space-5);
  color: var(--state-failure);
}

.filters {
  display: flex;
  align-items: center;
  gap: var(--space-4);
}

.search {
  position: relative;
  flex: 1;
  max-width: 360px;
}

.search :deep(.control) {
  padding-left: 34px;
}

.search-icon {
  position: absolute;
  top: 50%;
  left: 12px;
  z-index: 1;
  color: var(--text-secondary);
  transform: translateY(-50%);
  pointer-events: none;
}

.only-selected {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
}

.table-wrap {
  overflow-x: auto;
}

.table {
  min-width: 820px;
}

.col-check {
  width: 40px;
}

.col-check :deep(.checkbox) {
  display: flex;
}

.col-order {
  width: 56px;
}

.col-name {
  min-width: 140px;
  font-weight: 500;
}

.col-move {
  width: 100px;
}

.tags,
.move {
  display: inline-flex;
  flex-wrap: wrap;
  gap: var(--space-1);
}

.move {
  flex-wrap: nowrap;
}

.empty-link {
  color: var(--action-primary);
  font: var(--font-label);
}
</style>

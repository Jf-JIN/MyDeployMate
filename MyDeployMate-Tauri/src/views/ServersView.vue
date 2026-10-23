<script setup lang="ts">
import {
  Pencil,
  PlugZap,
  Plus,
  Search,
  SearchX,
  Server as ServerIcon,
  Shield,
  Trash2,
} from '@lucide/vue'
import { computed, nextTick, ref } from 'vue'

import EmptyState from '@/components/EmptyState.vue'
import PageLayout from '@/components/PageLayout.vue'
import { describeReference } from '@/components/references'
import ServerDialog from '@/components/servers/ServerDialog.vue'
import { hostPort } from '@/components/servers/serverForm'
import UiButton from '@/components/ui/UiButton.vue'
import UiCheckbox from '@/components/ui/UiCheckbox.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiTag from '@/components/ui/UiTag.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { cloneWorkspace } from '@/composables/workspaceDraft'
import { call, toAppError, type Reference, type Server } from '@/ipc'
import { useToastStore } from '@/stores/toast'
import { useWorkspaceStore } from '@/stores/workspace'

// P06 server list, phase 3.3 (IXD-15, IXD-20). Connection tests arrive with SSH in phase 5, so
// every server shows "未测试" and the test actions explain why they are unavailable.
const store = useWorkspaceStore()
const toast = useToastStore()

const TEST_LATER = '连接测试将在阶段 5 接入'

const query = ref('')
const environmentFilter = ref('')
const statusFilter = ref<'' | 'untested'>('')

const environmentOptions = computed(() => [
  { value: '', label: '全部环境' },
  ...store.environments.map((e) => ({ value: e.id, label: e.name })),
])
const statusOptions = [
  { value: '' as const, label: '全部状态' },
  { value: 'untested' as const, label: '未测试' },
]

function environment(id: string) {
  return store.environments.find((e) => e.id === id)
}

const sorted = computed(() =>
  [...store.servers].sort((a, b) => a.name.localeCompare(b.name, 'zh-CN', { numeric: true })),
)

const visible = computed(() => {
  const q = query.value.trim().toLowerCase()
  return sorted.value.filter((s) => {
    if (environmentFilter.value && s.environmentId !== environmentFilter.value) return false
    if (!q) return true
    return [s.name, s.host, ...s.tags].some((text) => text.toLowerCase().includes(q))
  })
})

const filtered = computed(() => !!query.value.trim() || !!environmentFilter.value)

function clearFilters() {
  query.value = ''
  environmentFilter.value = ''
  statusFilter.value = ''
}

// ---- Selection for batch test --------------------------------------------------------------

const selected = ref(new Set<string>())
const selectedVisible = computed(() => visible.value.filter((s) => selected.value.has(s.id)))
const allSelected = computed(
  () => visible.value.length > 0 && selectedVisible.value.length === visible.value.length,
)

function toggle(id: string, on: boolean) {
  const next = new Set(selected.value)
  if (on) next.add(id)
  else next.delete(id)
  selected.value = next
}

function toggleAll(on: boolean) {
  selected.value = on ? new Set(visible.value.map((s) => s.id)) : new Set()
}

const batchReason = computed(() =>
  selectedVisible.value.length ? TEST_LATER : '先勾选要测试的服务器',
)

// ---- Create and edit -----------------------------------------------------------------------

const editing = ref<Server | null>(null)
const isNew = ref(false)
const highlighted = ref<string | null>(null)
const listError = ref<string | null>(null)

async function create() {
  const env = environmentFilter.value || store.environments[0]?.id
  if (!env) return
  isNew.value = true
  editing.value = await call('new_server', { environmentId: env })
}

function edit(server: Server) {
  isNew.value = false
  editing.value = server
}

// IXD-15: after saving, show the row (clearing filters that hide it) and mark it briefly.
async function onSaved(server: Server) {
  const created = isNew.value
  editing.value = null
  toast.show(created ? `已新增 ${server.name}` : `已保存 ${server.name}`)
  if (!visible.value.some((s) => s.id === server.id)) clearFilters()
  highlighted.value = server.id
  await nextTick()
  document
    .querySelector<HTMLElement>(`[data-server-id="${server.id}"]`)
    ?.scrollIntoView({ block: 'nearest' })
  setTimeout(() => {
    if (highlighted.value === server.id) highlighted.value = null
  }, 2000)
}

// ---- Delete (IXD-20) -----------------------------------------------------------------------

const deleting = ref<Server | null>(null)
const deleteRefs = ref<Reference[]>([])
const deleteBusy = ref(false)

async function requestDelete(server: Server) {
  if (!store.workspace) return
  listError.value = null
  deleteRefs.value = await call('server_references', {
    draft: cloneWorkspace(store.workspace),
    serverId: server.id,
  })
  deleting.value = server
}

async function applyDelete() {
  const server = deleting.value
  if (!server || !store.workspace) return
  deleteBusy.value = true
  try {
    const result = await call('remove_server', {
      draft: cloneWorkspace(store.workspace),
      serverId: server.id,
    })
    await store.save(result.workspace)
    toggle(server.id, false)
    toast.show(`已删除 ${server.name}`)
  } catch (e) {
    listError.value = toAppError(e).message
  } finally {
    deleteBusy.value = false
    deleting.value = null
  }
}
</script>

<template>
  <PageLayout title="服务器">
    <template #actions>
      <UiButton :icon="PlugZap" :disabled-reason="batchReason">
        批量测试{{ selectedVisible.length ? `（${selectedVisible.length}）` : '' }}
      </UiButton>
      <UiButton variant="primary" :icon="Plus" @click="create">新增服务器</UiButton>
    </template>

    <div class="servers">
      <UiNotice v-if="listError" tone="failure" dismissible @dismiss="listError = null">
        {{ listError }}
      </UiNotice>

      <template v-if="store.servers.length">
        <div class="filters" role="search">
          <div class="search">
            <Search class="search-icon" :size="16" :stroke-width="1.75" aria-hidden="true" />
            <UiTextInput
              v-model="query"
              placeholder="搜索名称、地址或标签"
              aria-label="搜索服务器"
            />
          </div>
          <UiSelect
            v-model="environmentFilter"
            class="filter"
            :options="environmentOptions"
            aria-label="按环境筛选"
          />
          <UiSelect
            v-model="statusFilter"
            class="filter"
            :options="statusOptions"
            aria-label="按测试状态筛选"
          />
          <span class="count num">{{ visible.length }} / {{ store.servers.length }} 台</span>
        </div>

        <div class="table-wrap">
          <table v-if="visible.length" class="table">
            <thead>
              <tr>
                <th scope="col" class="col-check">
                  <UiCheckbox
                    :model-value="allSelected"
                    :indeterminate="!allSelected && selectedVisible.length > 0"
                    label="全选"
                    hide-label
                    @update:model-value="toggleAll"
                  />
                </th>
                <th scope="col" class="col-name">名称</th>
                <th scope="col" class="col-host">地址</th>
                <th scope="col">登录用户</th>
                <th scope="col">环境</th>
                <th scope="col">标签</th>
                <th scope="col" class="col-status">最近测试</th>
                <th scope="col" class="col-actions"><span class="sr-only">操作</span></th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="server in visible"
                :key="server.id"
                :data-server-id="server.id"
                :class="{ 'row--new': highlighted === server.id }"
              >
                <td class="col-check">
                  <UiCheckbox
                    :model-value="selected.has(server.id)"
                    :label="`选择 ${server.name}`"
                    hide-label
                    @update:model-value="toggle(server.id, $event)"
                  />
                </td>
                <td class="col-name">{{ server.name }}</td>
                <td class="col-host">
                  <code>{{ hostPort(server.host, server.port) }}</code>
                </td>
                <td>{{ server.user }}</td>
                <td>
                  <span class="env">
                    {{ store.environmentName(server.environmentId) }}
                    <UiTag
                      v-if="environment(server.environmentId)?.protected"
                      tone="warning"
                      :icon="Shield"
                    >
                      保护
                    </UiTag>
                  </span>
                </td>
                <td>
                  <span class="tags">
                    <UiTag v-for="tag in server.tags" :key="tag">{{ tag }}</UiTag>
                  </span>
                </td>
                <td class="col-status"><UiTag>未测试</UiTag></td>
                <td class="col-actions">
                  <span class="actions">
                    <UiIconButton
                      :icon="Pencil"
                      :label="`编辑 ${server.name}`"
                      @click="edit(server)"
                    />
                    <UiIconButton
                      :icon="PlugZap"
                      :label="`测试 ${server.name}`"
                      :disabled-reason="TEST_LATER"
                    />
                    <UiIconButton
                      :icon="Trash2"
                      :label="`删除 ${server.name}`"
                      @click="requestDelete(server)"
                    />
                  </span>
                </td>
              </tr>
            </tbody>
          </table>
          <EmptyState v-else title="没有符合条件的服务器" :icon="SearchX" compact>
            <UiButton v-if="filtered" compact @click="clearFilters">清除筛选</UiButton>
          </EmptyState>
        </div>
      </template>

      <EmptyState
        v-else
        title="尚无服务器"
        detail="添加要部署到的 Linux 服务器，之后在项目里按环境勾选为目标。"
        :icon="ServerIcon"
      >
        <UiButton variant="primary" :icon="Plus" @click="create">新增服务器</UiButton>
      </EmptyState>
    </div>

    <ServerDialog :server="editing" :is-new="isNew" @cancel="editing = null" @saved="onSaved" />

    <UiConfirm
      :open="!!deleting"
      :title="`删除服务器 ${deleting?.name ?? ''}`"
      confirm-label="删除配置并解除绑定"
      danger
      :busy="deleteBusy"
      @cancel="deleting = null"
      @confirm="applyDelete"
    >
      <template v-if="deleteRefs.length">
        <p>以下项目绑定了该服务器，删除后会一并解除：</p>
        <ul class="refs">
          <li v-for="(r, i) in deleteRefs" :key="i">{{ describeReference(r) }}</li>
        </ul>
      </template>
      <p v-else>没有项目绑定该服务器。</p>
      <p class="muted">运行历史会保留；不会删除服务器上的任何数据。</p>
    </UiConfirm>
  </PageLayout>
</template>

<style scoped>
.servers {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.filters {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}

.search {
  position: relative;
  flex: 1;
  min-width: 220px;
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

.filter {
  width: 160px;
}

.count {
  margin-left: auto;
  color: var(--text-secondary);
  font: var(--font-caption);
}

/* UI-13: the table scrolls sideways on its own, never the window. */
.table-wrap {
  overflow-x: auto;
}

.table {
  min-width: 860px;
}

.col-check {
  width: 40px;
}

.col-check :deep(.checkbox) {
  display: flex;
}

.col-name {
  min-width: 140px;
  font-weight: 600;
}

.col-host {
  min-width: 160px;
}

.col-status {
  width: 96px;
}

.col-actions {
  width: 124px;
}

.env,
.tags,
.actions {
  display: inline-flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1) var(--space-2);
}

.actions {
  flex-wrap: nowrap;
  gap: var(--space-1);
}

.row--new td {
  background: var(--surface-selected);
  transition: background 400ms;
}

.refs {
  margin: var(--space-1) 0 0;
  padding-left: var(--space-5);
}

.muted {
  color: var(--text-secondary);
}
</style>

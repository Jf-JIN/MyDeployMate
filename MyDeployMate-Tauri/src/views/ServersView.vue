<script setup lang="ts">
import {
  CircleStop,
  Pencil,
  PlugZap,
  Plus,
  Search,
  SearchX,
  Server as ServerIcon,
  Shield,
  Trash2,
} from '@lucide/vue'
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import CredentialDialog from '@/components/credentials/CredentialDialog.vue'
import EmptyState from '@/components/EmptyState.vue'
import PageLayout from '@/components/PageLayout.vue'
import { describeReference } from '@/components/references'
import HostTrustDialog from '@/components/servers/HostTrustDialog.vue'
import ServerDialog from '@/components/servers/ServerDialog.vue'
import { hostPort } from '@/components/servers/serverForm'
import {
  fixAction,
  formatDuration,
  formatTime,
  stageLabel,
  testStatus,
} from '@/components/servers/testStatus'
import UiButton from '@/components/ui/UiButton.vue'
import UiCheckbox from '@/components/ui/UiCheckbox.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiTag from '@/components/ui/UiTag.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { cloneWorkspace } from '@/composables/workspaceDraft'
import { purposeLabel } from '@/components/credentials/credentialLabels'
import {
  call,
  toAppError,
  type ConnectionTest,
  type Reference,
  type SavedCredential,
  type Server,
} from '@/ipc'
import { useConnectionTestStore } from '@/stores/connectionTests'
import { useToastStore } from '@/stores/toast'
import { useWorkspaceStore } from '@/stores/workspace'

// P06 server list (IXD-15, IXD-20) with single and batch connection tests (IXD-18) and first
// trust of a server key (D02, IXD-19).
const store = useWorkspaceStore()
const toast = useToastStore()
const tests = useConnectionTestStore()
const router = useRouter()

// Results the backend no longer considers valid (connection fields edited) disappear on reload.
onMounted(() => void tests.load())
watch(
  () => store.workspace?.revision,
  () => void tests.load(),
)

const query = ref('')
const environmentFilter = ref('')
const statusFilter = ref<'' | 'untested' | 'succeeded' | 'failed'>('')

const environmentOptions = computed(() => [
  { value: '', label: '全部环境' },
  ...store.environments.map((e) => ({ value: e.id, label: e.name })),
])
const statusOptions = [
  { value: '' as const, label: '全部状态' },
  { value: 'untested' as const, label: '未测试' },
  { value: 'succeeded' as const, label: '连接正常' },
  { value: 'failed' as const, label: '测试失败' },
]

function statusOf(server: Server) {
  return testStatus(tests.results[server.id], !!tests.running[server.id])
}

function matchesStatus(server: Server): boolean {
  const outcome = tests.results[server.id]?.outcome.status
  switch (statusFilter.value) {
    case '':
      return true
    case 'untested':
      return !outcome
    case 'succeeded':
      return outcome === 'succeeded'
    case 'failed':
      return outcome === 'failed'
  }
}

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
    if (!matchesStatus(s)) return false
    if (!q) return true
    return [s.name, s.host, ...s.tags].some((text) => text.toLowerCase().includes(q))
  })
})

const filtered = computed(
  () => !!query.value.trim() || !!environmentFilter.value || !!statusFilter.value,
)

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

const batchReason = computed(() => (selectedVisible.value.length ? null : '先勾选要测试的服务器'))

// ---- Connection tests (IXD-18) -------------------------------------------------------------

function runTest(server: Server) {
  void tests.test(server.id, server)
}

function runBatch() {
  void tests.testAll(selectedVisible.value)
}

const detail = ref<{ server: Server; test: ConnectionTest } | null>(null)

function showDetail(server: Server) {
  const test = tests.results[server.id]
  if (test) detail.value = { server, test }
}

const detailError = computed(() => {
  const outcome = detail.value?.test.outcome
  return outcome?.status === 'failed' ? outcome.error : null
})

function applyFix() {
  const error = detailError.value
  const server = detail.value?.server
  detail.value = null
  if (!error || !server) return
  if (fixAction(error) === 'trustManagement') openTrustManagement()
  else edit(server)
}

function openTrustManagement() {
  tests.keyChange = null
  void router.push({ name: 'settings', hash: '#trust' })
}

const credentialBusy = ref(false)
async function answerCredential(secret: string | null, remember = false) {
  credentialBusy.value = true
  try {
    await tests.answerCredential(secret, remember)
  } finally {
    credentialBusy.value = false
  }
}

const trustBusy = ref(false)
async function answerTrust(trust: boolean) {
  trustBusy.value = true
  try {
    await tests.answerTrust(trust)
  } finally {
    trustBusy.value = false
  }
}

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
const deleteCredentials = ref<SavedCredential[]>([])
const deleteBusy = ref(false)

async function requestDelete(server: Server) {
  if (!store.workspace) return
  listError.value = null
  deleteRefs.value = await call('server_references', {
    draft: cloneWorkspace(store.workspace),
    serverId: server.id,
  })
  try {
    const saved = await call('saved_credentials')
    deleteCredentials.value = saved.filter((c) => c.serverId === server.id)
  } catch {
    deleteCredentials.value = []
  }
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
      <template v-if="tests.batch">
        <span class="batch-progress num" role="status">
          批量测试 {{ tests.batch.done }} / {{ tests.batch.total }}
        </span>
        <UiButton :icon="CircleStop" :disabled="tests.batch.cancelled" @click="tests.cancelBatch()">
          取消批量测试
        </UiButton>
      </template>
      <UiButton v-else :icon="PlugZap" :disabled-reason="batchReason" @click="runBatch">
        批量测试{{ selectedVisible.length ? `（${selectedVisible.length}）` : '' }}
      </UiButton>
      <UiButton variant="primary" :icon="Plus" @click="create">新增服务器</UiButton>
    </template>

    <div class="servers">
      <UiNotice v-if="listError" tone="failure" dismissible @dismiss="listError = null">
        {{ listError }}
      </UiNotice>
      <UiNotice v-if="tests.error" tone="failure" dismissible @dismiss="tests.error = null">
        {{ tests.error }}
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
                class="row"
                :class="{ 'row--new': highlighted === server.id }"
                @click="edit(server)"
              >
                <td class="col-check" @click.stop>
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
                <td class="col-status" @click.stop>
                  <button
                    v-if="tests.results[server.id] && !tests.running[server.id]"
                    type="button"
                    class="status-button"
                    :title="`查看 ${server.name} 的测试详情`"
                    @click="showDetail(server)"
                  >
                    <UiTag :tone="statusOf(server).tone" :icon="statusOf(server).icon">
                      {{ statusOf(server).label }}
                    </UiTag>
                    <span class="status-time num">
                      {{ formatTime(tests.results[server.id]!.startedAtMs) }}
                    </span>
                  </button>
                  <span v-else class="status-plain">
                    <UiTag
                      :tone="statusOf(server).tone"
                      :icon="statusOf(server).icon"
                      :spin="statusOf(server).spin"
                    >
                      {{ statusOf(server).label }}
                    </UiTag>
                    <span v-if="tests.running[server.id]" class="status-time num">
                      {{ formatTime(tests.running[server.id]!.startedAtMs) }} 开始
                    </span>
                  </span>
                </td>
                <td class="col-actions" @click.stop>
                  <span class="actions">
                    <UiIconButton
                      :icon="Pencil"
                      :label="`编辑 ${server.name}`"
                      @click="edit(server)"
                    />
                    <UiIconButton
                      v-if="tests.running[server.id]"
                      :icon="CircleStop"
                      :label="`取消测试 ${server.name}`"
                      @click="tests.cancel(server.id)"
                    />
                    <UiIconButton
                      v-else
                      :icon="PlugZap"
                      :label="`测试 ${server.name}`"
                      :disabled-reason="tests.batch ? '批量测试进行中' : null"
                      @click="runTest(server)"
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

    <HostTrustDialog
      :open="!!tests.pendingTrust"
      mode="first"
      :server-name="tests.pendingTrust?.serverName ?? ''"
      :user="tests.pendingTrust?.user ?? ''"
      :endpoint="tests.pendingTrust?.endpoint ?? null"
      :presented="tests.pendingTrust?.presented ?? null"
      :busy="trustBusy"
      @trust="answerTrust(true)"
      @cancel="answerTrust(false)"
    />
    <CredentialDialog
      :prompt="tests.pendingTrust ? null : tests.pendingCredential"
      :busy="credentialBusy"
      @submit="answerCredential"
      @cancel="answerCredential(null)"
    />
    <HostTrustDialog
      :open="!tests.pendingTrust && !!tests.keyChange"
      mode="changed"
      :server-name="tests.keyChange?.serverName ?? ''"
      :user="tests.keyChange?.user ?? ''"
      :endpoint="tests.keyChange?.endpoint ?? null"
      :presented="tests.keyChange?.presented ?? null"
      :known="tests.keyChange?.known ?? []"
      @cancel="tests.keyChange = null"
      @manage="openTrustManagement"
    />

    <UiDialog
      :open="!!detail"
      :title="`测试详情：${detail?.server.name ?? ''}`"
      width="credential"
      backdrop-cancels
      @cancel="detail = null"
    >
      <dl v-if="detail" class="detail">
        <dt>服务器</dt>
        <dd>
          {{ detail.server.user }}@<code>{{
            hostPort(detail.server.host, detail.server.port)
          }}</code>
        </dd>
        <dt>开始时间</dt>
        <dd class="num">{{ new Date(detail.test.startedAtMs).toLocaleString('zh-CN') }}</dd>
        <dt>耗时</dt>
        <dd class="num">{{ formatDuration(detail.test.durationMs) }}</dd>
        <dt>结果</dt>
        <dd>
          <UiTag :tone="statusOf(detail.server).tone" :icon="statusOf(detail.server).icon">
            {{ statusOf(detail.server).label }}
          </UiTag>
        </dd>
        <template v-if="detail.test.outcome.status === 'succeeded'">
          <dt>系统</dt>
          <dd>
            <code>{{ detail.test.outcome.system }}</code>
          </dd>
        </template>
        <template v-if="detailError">
          <dt>阶段</dt>
          <dd>{{ stageLabel(detailError.stage) }}</dd>
          <dt>原因</dt>
          <dd class="detail-message">{{ detailError.message }}</dd>
        </template>
      </dl>
      <template #footer>
        <UiButton @click="detail = null">关闭</UiButton>
        <UiButton v-if="detailError && fixAction(detailError)" variant="tonal" @click="applyFix">
          {{ fixAction(detailError) === 'trustManagement' ? '前往信任管理' : '编辑服务器' }}
        </UiButton>
      </template>
    </UiDialog>

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
      <template v-if="deleteCredentials.length">
        <p>该服务器的已保存凭据会一并删除：</p>
        <ul class="refs">
          <li v-for="c in deleteCredentials" :key="purposeLabel(c.purpose) + c.user">
            {{ c.user }} · {{ purposeLabel(c.purpose) }}
          </li>
        </ul>
      </template>
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
  width: 170px;
}

.row {
  cursor: pointer;
}

/* The whole row opens the server form, like the project list; the edit button is the
   keyboard way in. */
.row:hover > td {
  background: var(--surface-hover);
}

.row:hover .col-name {
  color: var(--action-primary);
}

.status-button,
.status-plain {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
}

.status-button {
  padding: 2px;
  border: 0;
  border-radius: var(--radius-control);
  background: transparent;
  color: inherit;
  font: inherit;
  cursor: pointer;
}

.status-button:hover .status-time {
  text-decoration: underline;
}

.status-time {
  color: var(--text-secondary);
  font: var(--font-caption);
  white-space: nowrap;
}

.batch-progress {
  color: var(--text-secondary);
}

.detail {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: var(--space-2) var(--space-4);
  margin: 0;
}

.detail dt {
  color: var(--text-secondary);
}

.detail dd {
  margin: 0;
  overflow-wrap: anywhere;
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

<script setup lang="ts">
import { RefreshCw, ShieldAlert, ShieldCheck, Trash2 } from '@lucide/vue'
import { computed, nextTick, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'

import { hostPort } from '@/components/servers/serverForm'
import { formatTime } from '@/components/servers/testStatus'
import UiButton from '@/components/ui/UiButton.vue'
import UiCheckbox from '@/components/ui/UiCheckbox.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import { call, toAppError, type HostEndpoint, type HostKey, type TrustedHost } from '@/ipc'
import { useToastStore } from '@/stores/toast'
import { useWorkspaceStore } from '@/stores/workspace'

// P10 › 主机信任 (IXD-19, IXD-36, SRV-03). Lists every recorded server key. An update reads the
// key the server presents now, shows it next to the recorded one, and needs an explicit
// confirmation; removing a record makes the next connection ask for first trust again.
const store = useWorkspaceStore()
const toast = useToastStore()
const route = useRoute()
const root = ref<HTMLElement | null>(null)

const hosts = ref<TrustedHost[]>([])
const error = ref<string | null>(null)

async function load() {
  try {
    hosts.value = await call('trusted_hosts')
  } catch (e) {
    error.value = toAppError(e).message
  }
}

onMounted(async () => {
  await load()
  if (route.hash === '#trust') {
    await nextTick()
    root.value?.scrollIntoView({ block: 'start' })
  }
})

/** Same normalisation as the backend for the common cases: case and IPv6 brackets. */
function sameEndpoint(host: string, port: number, endpoint: HostEndpoint) {
  const h = host
    .trim()
    .replace(/^\[(.*)\]$/, '$1')
    .toLowerCase()
  return port === endpoint.port && h === endpoint.host.toLowerCase()
}

function serversAt(endpoint: HostEndpoint) {
  return store.servers.filter((s) => sameEndpoint(s.host, s.port, endpoint))
}

function key(h: TrustedHost) {
  return `${h.endpoint.host}|${h.endpoint.port}|${h.key.fingerprint}`
}

// ---- Update ---------------------------------------------------------------------------------

const updating = ref<{ endpoint: HostEndpoint; known: HostKey[] } | null>(null)
const fetched = ref<HostKey | null>(null)
const fetchError = ref<string | null>(null)
const fetching = ref(false)
const checked = ref(false)
const saving = ref(false)

const unchanged = computed(
  () =>
    !!fetched.value &&
    !!updating.value?.known.some((k) => k.fingerprint === fetched.value?.fingerprint),
)

async function startUpdate(endpoint: HostEndpoint) {
  updating.value = {
    endpoint,
    known: hosts.value
      .filter((h) => h.endpoint.host === endpoint.host && h.endpoint.port === endpoint.port)
      .map((h) => h.key),
  }
  fetched.value = null
  fetchError.value = null
  checked.value = false
  fetching.value = true
  const timeoutSec = serversAt(endpoint)[0]?.connectTimeoutSec ?? 10
  try {
    fetched.value = await call('fetch_host_key', { endpoint, timeoutSec })
  } catch (e) {
    fetchError.value = toAppError(e).message
  } finally {
    fetching.value = false
  }
}

async function applyUpdate() {
  if (!updating.value || !fetched.value) return
  saving.value = true
  try {
    await call('replace_trusted_host', { endpoint: updating.value.endpoint, key: fetched.value })
    toast.show(
      `已更新 ${hostPort(updating.value.endpoint.host, updating.value.endpoint.port)} 的信任记录`,
    )
    updating.value = null
    await load()
  } catch (e) {
    fetchError.value = toAppError(e).message
  } finally {
    saving.value = false
  }
}

// ---- Remove ---------------------------------------------------------------------------------

const removing = ref<TrustedHost | null>(null)
const removeBusy = ref(false)

async function applyRemove() {
  const host = removing.value
  if (!host) return
  removeBusy.value = true
  try {
    await call('remove_trusted_host', { endpoint: host.endpoint })
    toast.show(`已删除 ${hostPort(host.endpoint.host, host.endpoint.port)} 的信任记录`)
    await load()
  } catch (e) {
    error.value = toAppError(e).message
  } finally {
    removeBusy.value = false
    removing.value = null
  }
}
</script>

<template>
  <section id="trust" ref="root" class="section" aria-labelledby="trust-title">
    <header class="section-head">
      <h2 id="trust-title">主机信任</h2>
      <p class="section-hint">
        首次连接时核对并信任的服务器密钥。密钥变化时连接会被阻止，只能在这里重新获取并确认后更新。
      </p>
    </header>
    <UiNotice v-if="error" tone="failure" dismissible @dismiss="error = null">{{ error }}</UiNotice>
    <div v-if="hosts.length" class="table-wrap">
      <table class="table">
        <thead>
          <tr>
            <th scope="col">地址</th>
            <th scope="col">服务器</th>
            <th scope="col">密钥</th>
            <th scope="col" class="col-time">信任时间</th>
            <th scope="col" class="col-actions"><span class="sr-only">操作</span></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="h in hosts" :key="key(h)">
            <td>
              <code>{{ hostPort(h.endpoint.host, h.endpoint.port) }}</code>
            </td>
            <td>
              <span v-if="serversAt(h.endpoint).length">
                {{
                  serversAt(h.endpoint)
                    .map((s) => s.name)
                    .join('、')
                }}
              </span>
              <span v-else class="muted">无</span>
            </td>
            <td>
              <span class="alg">{{ h.key.algorithm }}</span>
              <code class="fp">{{ h.key.fingerprint }}</code>
            </td>
            <td class="col-time">{{ formatTime(h.trustedAtMs) }}</td>
            <td class="col-actions">
              <span class="actions">
                <UiIconButton
                  :icon="RefreshCw"
                  :label="`重新获取并更新 ${hostPort(h.endpoint.host, h.endpoint.port)}`"
                  @click="startUpdate(h.endpoint)"
                />
                <UiIconButton
                  :icon="Trash2"
                  :label="`删除 ${hostPort(h.endpoint.host, h.endpoint.port)} 的信任记录`"
                  @click="removing = h"
                />
              </span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <p v-else class="muted">尚无信任记录。首次测试或连接服务器时会请你核对指纹。</p>

    <UiDialog
      :open="!!updating"
      :title="`更新信任：${updating ? hostPort(updating.endpoint.host, updating.endpoint.port) : ''}`"
      width="server"
      :cancellable="!saving"
      @cancel="updating = null"
    >
      <div v-if="updating" class="update">
        <section class="key" aria-label="已记录的密钥">
          <h3><ShieldCheck :size="16" :stroke-width="1.75" aria-hidden="true" /> 已记录</h3>
          <div v-for="k in updating.known" :key="k.fingerprint" class="key-row">
            <span class="alg">{{ k.algorithm }}</span>
            <code class="fp">{{ k.fingerprint }}</code>
          </div>
        </section>
        <section
          class="key"
          :class="{ 'key--new': fetched && !unchanged }"
          aria-label="服务器当前出示的密钥"
        >
          <h3>
            <ShieldAlert
              v-if="fetched && !unchanged"
              :size="16"
              :stroke-width="1.75"
              aria-hidden="true"
            />
            服务器当前出示
          </h3>
          <p v-if="fetching" class="muted" role="status">正在连接服务器读取密钥…</p>
          <UiNotice v-else-if="fetchError" tone="failure">{{ fetchError }}</UiNotice>
          <div v-else-if="fetched" class="key-row">
            <span class="alg">{{ fetched.algorithm }}</span>
            <code class="fp">{{ fetched.fingerprint }}</code>
          </div>
        </section>
        <UiNotice v-if="unchanged" tone="success">指纹与已记录的一致，无需更新。</UiNotice>
        <template v-else-if="fetched">
          <UiNotice tone="warning">
            更新后将只信任新的密钥。请先通过可信渠道（如服务器管理员或控制台）核对新指纹。
          </UiNotice>
          <UiCheckbox v-model="checked" label="我已核对新指纹，确认服务器确实更换了密钥" />
        </template>
      </div>
      <template #footer>
        <UiButton autofocus :disabled="saving" @click="updating = null">
          {{ unchanged ? '关闭' : '取消' }}
        </UiButton>
        <UiButton
          v-if="fetched && !unchanged"
          variant="danger"
          :busy="saving"
          :disabled-reason="checked ? null : '请先勾选已核对新指纹'"
          @click="applyUpdate"
        >
          更新信任
        </UiButton>
      </template>
    </UiDialog>

    <UiConfirm
      :open="!!removing"
      :title="`删除信任记录 ${removing ? hostPort(removing.endpoint.host, removing.endpoint.port) : ''}`"
      confirm-label="删除"
      danger
      :busy="removeBusy"
      @cancel="removing = null"
      @confirm="applyRemove"
    >
      <p>删除后，下次连接该地址时会重新请你核对服务器指纹。</p>
    </UiConfirm>
  </section>
</template>

<style scoped>
.section {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  scroll-margin-top: var(--space-4);
}

.section-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1) var(--space-3);
}

.section-hint {
  flex: 1;
  min-width: 240px;
  margin: 0;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.table-wrap {
  overflow-x: auto;
}

.table {
  min-width: 760px;
}

.table td {
  height: auto;
  padding-top: var(--space-2);
  padding-bottom: var(--space-2);
}

.col-time {
  width: 120px;
  white-space: nowrap;
}

.col-actions {
  width: 88px;
}

.actions {
  display: inline-flex;
  gap: var(--space-1);
}

.muted {
  margin: 0;
  color: var(--text-secondary);
}

.alg {
  display: block;
  color: var(--text-secondary);
  font: var(--font-caption);
}

/* IXD-19: the whole fingerprint stays visible and selectable. */
.fp {
  font: var(--font-code);
  overflow-wrap: anywhere;
  user-select: all;
}

.update {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.key {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-control);
}

.key h3 {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  margin: 0;
  font: var(--font-label);
}

.key--new {
  border-color: var(--state-failure);
}

.key-row {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
</style>

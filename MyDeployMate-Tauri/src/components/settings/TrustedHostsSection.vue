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
import { useT } from '@/i18n'
import { errorText } from '@/i18n/message'
import { call, type HostEndpoint, type HostKey, type TrustedHost } from '@/ipc'
import { useToastStore } from '@/stores/toast'
import { useWorkspaceStore } from '@/stores/workspace'

const t = useT()

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
    error.value = errorText(e)
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
    fetchError.value = errorText(e)
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
      t('settings.trust.updated', {
        host: hostPort(updating.value.endpoint.host, updating.value.endpoint.port),
      }),
    )
    updating.value = null
    await load()
  } catch (e) {
    fetchError.value = errorText(e)
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
    toast.show(
      t('settings.trust.removed', { host: hostPort(host.endpoint.host, host.endpoint.port) }),
    )
    await load()
  } catch (e) {
    error.value = errorText(e)
  } finally {
    removeBusy.value = false
    removing.value = null
  }
}
</script>

<template>
  <section id="trust" ref="root" class="section" aria-labelledby="trust-title">
    <header class="section-head">
      <h2 id="trust-title">{{ t('settings.trust.title') }}</h2>
      <p class="section-hint">
        {{ t('settings.trust.hint') }}
      </p>
    </header>
    <UiNotice v-if="error" tone="failure" dismissible @dismiss="error = null">{{ error }}</UiNotice>
    <div v-if="hosts.length" class="table-wrap">
      <table class="table">
        <thead>
          <tr>
            <th scope="col">{{ t('common.address') }}</th>
            <th scope="col">{{ t('common.server') }}</th>
            <th scope="col">{{ t('settings.trust.key') }}</th>
            <th scope="col" class="col-time">{{ t('settings.trust.trustedAt') }}</th>
            <th scope="col" class="col-actions">
              <span class="sr-only">{{ t('common.actions') }}</span>
            </th>
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
                    .join(t('common.listSeparator'))
                }}
              </span>
              <span v-else class="muted">{{ t('common.none') }}</span>
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
                  :label="
                    t('settings.trust.refetch', {
                      host: hostPort(h.endpoint.host, h.endpoint.port),
                    })
                  "
                  @click="startUpdate(h.endpoint)"
                />
                <UiIconButton
                  :icon="Trash2"
                  :label="
                    t('settings.trust.removeItem', {
                      host: hostPort(h.endpoint.host, h.endpoint.port),
                    })
                  "
                  @click="removing = h"
                />
              </span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <p v-else class="muted">{{ t('settings.trust.empty') }}</p>

    <UiDialog
      :open="!!updating"
      :title="
        t('settings.trust.updateTitle', {
          host: updating ? hostPort(updating.endpoint.host, updating.endpoint.port) : '',
        })
      "
      width="server"
      :cancellable="!saving"
      @cancel="updating = null"
    >
      <div v-if="updating" class="update">
        <section class="key" :aria-label="t('trust.knownKeys')">
          <h3>
            <ShieldCheck :size="16" :stroke-width="1.75" aria-hidden="true" />
            {{ t('trust.known') }}
          </h3>
          <div v-for="k in updating.known" :key="k.fingerprint" class="key-row">
            <span class="alg">{{ k.algorithm }}</span>
            <code class="fp">{{ k.fingerprint }}</code>
          </div>
        </section>
        <section
          class="key"
          :class="{ 'key--new': fetched && !unchanged }"
          :aria-label="t('settings.trust.currentKeys')"
        >
          <h3>
            <ShieldAlert
              v-if="fetched && !unchanged"
              :size="16"
              :stroke-width="1.75"
              aria-hidden="true"
            />
            {{ t('settings.trust.current') }}
          </h3>
          <p v-if="fetching" class="muted" role="status">{{ t('settings.trust.fetching') }}</p>
          <UiNotice v-else-if="fetchError" tone="failure">{{ fetchError }}</UiNotice>
          <div v-else-if="fetched" class="key-row">
            <span class="alg">{{ fetched.algorithm }}</span>
            <code class="fp">{{ fetched.fingerprint }}</code>
          </div>
        </section>
        <UiNotice v-if="unchanged" tone="success">{{ t('settings.trust.unchanged') }}</UiNotice>
        <template v-else-if="fetched">
          <UiNotice tone="warning">
            {{ t('settings.trust.updateWarning') }}
          </UiNotice>
          <UiCheckbox v-model="checked" :label="t('settings.trust.checked')" />
        </template>
      </div>
      <template #footer>
        <UiButton autofocus :disabled="saving" @click="updating = null">
          {{ unchanged ? t('common.close') : t('common.cancel') }}
        </UiButton>
        <UiButton
          v-if="fetched && !unchanged"
          variant="danger"
          :busy="saving"
          :disabled-reason="checked ? null : t('settings.trust.checkFirst')"
          @click="applyUpdate"
        >
          {{ t('settings.trust.update') }}
        </UiButton>
      </template>
    </UiDialog>

    <UiConfirm
      :open="!!removing"
      :title="
        t('settings.trust.removeTitle', {
          host: removing ? hostPort(removing.endpoint.host, removing.endpoint.port) : '',
        })
      "
      :confirm-label="t('common.delete')"
      danger
      :busy="removeBusy"
      @cancel="removing = null"
      @confirm="applyRemove"
    >
      <p>{{ t('settings.trust.removeBody') }}</p>
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

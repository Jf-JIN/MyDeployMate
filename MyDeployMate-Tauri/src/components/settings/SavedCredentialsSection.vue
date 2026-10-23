<script setup lang="ts">
import { Trash2 } from '@lucide/vue'
import { onMounted, ref } from 'vue'

import { purposeLabel } from '@/components/credentials/credentialLabels'
import { formatTime } from '@/components/servers/testStatus'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import { useT } from '@/i18n'
import { call, toAppError, type SavedCredential } from '@/ipc'
import { useToastStore } from '@/stores/toast'
import { useWorkspaceStore } from '@/stores/workspace'

const t = useT()

// P10 › 已保存凭据 (SEC-03, IXD-36). Lists what is remembered in Windows Credential Manager,
// never the secrets, and clears entries one by one. A cleared credential is asked for again
// next time, with "remember" unticked.
const store = useWorkspaceStore()
const toast = useToastStore()

const saved = ref<SavedCredential[]>([])
const error = ref<string | null>(null)

async function load() {
  try {
    saved.value = await call('saved_credentials')
  } catch (e) {
    error.value = toAppError(e).message
  }
}
onMounted(load)

function serverName(id: string) {
  return store.servers.find((s) => s.id === id)?.name ?? null
}

function key(c: SavedCredential) {
  return `${c.serverId}|${c.user}|${purposeLabel(c.purpose)}`
}

const clearing = ref<SavedCredential | null>(null)
const busy = ref(false)

async function applyClear() {
  const c = clearing.value
  if (!c) return
  busy.value = true
  try {
    await call('forget_credential', {
      credential: { serverId: c.serverId, user: c.user, purpose: c.purpose },
    })
    toast.show(t('settings.credentials.cleared'))
    await load()
  } catch (e) {
    error.value = toAppError(e).message
  } finally {
    busy.value = false
    clearing.value = null
  }
}
</script>

<template>
  <section class="section" aria-labelledby="cred-title">
    <header class="section-head">
      <h2 id="cred-title">{{ t('settings.credentials.title') }}</h2>
      <p class="section-hint">
        {{ t('settings.credentials.hint') }}
      </p>
    </header>
    <UiNotice v-if="error" tone="failure" dismissible @dismiss="error = null">{{ error }}</UiNotice>
    <div v-if="saved.length" class="table-wrap">
      <table class="table">
        <thead>
          <tr>
            <th scope="col">{{ t('common.server') }}</th>
            <th scope="col">{{ t('common.user') }}</th>
            <th scope="col">{{ t('settings.credentials.purpose') }}</th>
            <th scope="col" class="col-time">{{ t('settings.credentials.savedAt') }}</th>
            <th scope="col" class="col-actions">
              <span class="sr-only">{{ t('common.actions') }}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="c in saved" :key="key(c)">
            <td>
              <span v-if="serverName(c.serverId)">{{ serverName(c.serverId) }}</span>
              <span v-else class="muted">{{ t('common.deletedServer') }}</span>
            </td>
            <td>{{ c.user }}</td>
            <td>{{ purposeLabel(c.purpose) }}</td>
            <td class="col-time">{{ formatTime(c.savedAtMs) }}</td>
            <td class="col-actions">
              <UiIconButton
                :icon="Trash2"
                :label="
                  t('settings.credentials.clearItem', {
                    server: serverName(c.serverId) ?? '',
                    purpose: purposeLabel(c.purpose),
                  })
                "
                @click="clearing = c"
              />
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <p v-else class="muted">{{ t('settings.credentials.empty') }}</p>

    <UiConfirm
      :open="!!clearing"
      :title="t('settings.credentials.clearTitle')"
      :confirm-label="t('settings.credentials.clear')"
      danger
      :busy="busy"
      @cancel="clearing = null"
      @confirm="applyClear"
    >
      <p v-if="clearing">
        {{
          t('settings.credentials.clearBody', {
            server: serverName(clearing.serverId) ?? t('common.deletedServer'),
            user: clearing.user,
            purpose: purposeLabel(clearing.purpose),
          })
        }}
      </p>
    </UiConfirm>
  </section>
</template>

<style scoped>
.section {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
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
  min-width: 640px;
}

.col-time {
  width: 120px;
  white-space: nowrap;
}

.col-actions {
  width: 48px;
}

.muted {
  margin: 0;
  color: var(--text-secondary);
}
</style>

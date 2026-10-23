<script setup lang="ts">
import { Trash2 } from '@lucide/vue'
import { onMounted, ref } from 'vue'

import { purposeLabel } from '@/components/credentials/credentialLabels'
import { formatTime } from '@/components/servers/testStatus'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import { call, toAppError, type SavedCredential } from '@/ipc'
import { useToastStore } from '@/stores/toast'
import { useWorkspaceStore } from '@/stores/workspace'

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
    toast.show('已清除保存的凭据')
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
      <h2 id="cred-title">已保存凭据</h2>
      <p class="section-hint">
        在凭据弹窗中勾选“记住”后保存到 Windows 凭据管理器的密码和口令。这里只显示用途，不显示内容。
      </p>
    </header>
    <UiNotice v-if="error" tone="failure" dismissible @dismiss="error = null">{{ error }}</UiNotice>
    <div v-if="saved.length" class="table-wrap">
      <table class="table">
        <thead>
          <tr>
            <th scope="col">服务器</th>
            <th scope="col">登录账号</th>
            <th scope="col">用途</th>
            <th scope="col" class="col-time">保存时间</th>
            <th scope="col" class="col-actions"><span class="sr-only">操作</span></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="c in saved" :key="key(c)">
            <td>
              <span v-if="serverName(c.serverId)">{{ serverName(c.serverId) }}</span>
              <span v-else class="muted">已删除的服务器</span>
            </td>
            <td>{{ c.user }}</td>
            <td>{{ purposeLabel(c.purpose) }}</td>
            <td class="col-time">{{ formatTime(c.savedAtMs) }}</td>
            <td class="col-actions">
              <UiIconButton
                :icon="Trash2"
                :label="`清除 ${serverName(c.serverId) ?? ''} ${purposeLabel(c.purpose)}`"
                @click="clearing = c"
              />
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <p v-else class="muted">尚无保存的凭据。</p>

    <UiConfirm
      :open="!!clearing"
      title="清除保存的凭据"
      confirm-label="清除"
      danger
      :busy="busy"
      @cancel="clearing = null"
      @confirm="applyClear"
    >
      <p v-if="clearing">
        清除 {{ serverName(clearing.serverId) ?? '已删除的服务器' }}（{{ clearing.user }}）的{{
          purposeLabel(clearing.purpose)
        }}。之后连接时会重新询问。
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

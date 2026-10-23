<script setup lang="ts">
import { ShieldAlert, ShieldCheck } from '@lucide/vue'
import { computed } from 'vue'

import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import type { HostEndpoint, HostKey } from '@/ipc'

// D02 (IXD-19, SRV-03). First trust asks the operator to compare the fingerprint, with the
// focus on "取消连接" so nothing is trusted by a stray Enter. A changed key only informs and
// blocks: no button here accepts it; updating goes through trust management.
const props = defineProps<{
  open: boolean
  mode: 'first' | 'changed'
  serverName: string
  user: string
  endpoint: HostEndpoint | null
  presented: HostKey | null
  known?: HostKey[]
  busy?: boolean
}>()

const emit = defineEmits<{ trust: []; cancel: []; manage: [] }>()

const title = computed(() =>
  props.mode === 'first' ? '确认服务器身份' : '服务器身份与已记录的不一致',
)
</script>

<template>
  <UiDialog
    :open="open"
    :title="title"
    width="server"
    :cancellable="!busy"
    @cancel="emit('cancel')"
  >
    <div v-if="endpoint && presented" class="trust">
      <UiNotice v-if="mode === 'changed'" tone="failure">
        服务器出示的密钥与已记录的不同，连接已停止。可能是服务器重装或更换了密钥，也可能有人在冒充该服务器。请先向服务器管理员核实。
      </UiNotice>
      <p v-else class="lead">
        首次连接 <strong>{{ serverName }}</strong
        >。请通过可信渠道（如服务器管理员或控制台）核对下面的指纹，一致后再信任。
      </p>

      <dl class="facts">
        <dt>地址</dt>
        <dd>
          <code>{{ endpoint.host }}</code>
        </dd>
        <dt>端口</dt>
        <dd class="num">{{ endpoint.port }}</dd>
        <dt>登录账号</dt>
        <dd>{{ user }}</dd>
      </dl>

      <section v-if="mode === 'changed'" class="key key--old" aria-label="已记录的密钥">
        <h3><ShieldCheck :size="16" :stroke-width="1.75" aria-hidden="true" /> 已记录</h3>
        <div v-for="k in known" :key="k.fingerprint" class="key-row">
          <span class="alg">{{ k.algorithm }}</span>
          <code class="fp">{{ k.fingerprint }}</code>
        </div>
      </section>
      <section
        class="key"
        :class="{ 'key--new': mode === 'changed' }"
        aria-label="服务器出示的密钥"
      >
        <h3 v-if="mode === 'changed'">
          <ShieldAlert :size="16" :stroke-width="1.75" aria-hidden="true" /> 本次出示
        </h3>
        <div class="key-row">
          <span class="alg">{{ presented.algorithm }}</span>
          <code class="fp">{{ presented.fingerprint }}</code>
        </div>
      </section>
      <p class="muted">信任只确认服务器身份，不代表信任它输出的任何内容。</p>
    </div>

    <template #footer>
      <template v-if="mode === 'first'">
        <UiButton autofocus :disabled="busy" @click="emit('cancel')">取消连接</UiButton>
        <UiButton variant="primary" :busy="busy" @click="emit('trust')"
          >已核对，信任并连接</UiButton
        >
      </template>
      <template v-else>
        <UiButton autofocus @click="emit('cancel')">关闭</UiButton>
        <UiButton variant="tonal" @click="emit('manage')">前往信任管理</UiButton>
      </template>
    </template>
  </UiDialog>
</template>

<style scoped>
.trust {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.lead,
.muted {
  margin: 0;
}

.muted {
  color: var(--text-secondary);
  font: var(--font-caption);
}

.facts {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: var(--space-2) var(--space-4);
  margin: 0;
}

.facts dt {
  color: var(--text-secondary);
}

.facts dd {
  margin: 0;
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

.alg {
  color: var(--text-secondary);
  font: var(--font-caption);
}

/* IXD-19: the whole fingerprint stays visible and selectable. */
.fp {
  font: var(--font-code);
  overflow-wrap: anywhere;
  user-select: all;
}
</style>

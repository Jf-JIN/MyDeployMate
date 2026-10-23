<script setup lang="ts">
import { ShieldAlert, ShieldCheck } from '@lucide/vue'
import { computed } from 'vue'

import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import { useT } from '@/i18n'
import type { HostEndpoint, HostKey } from '@/ipc'

const t = useT()

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
  props.mode === 'first' ? t('trust.firstTitle') : t('trust.changedTitle'),
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
        {{ t('trust.changedNotice') }}
      </UiNotice>
      <i18n-t v-else keypath="trust.firstLead" tag="p" class="lead">
        <template #server>
          <strong>{{ serverName }}</strong>
        </template>
      </i18n-t>

      <dl class="facts">
        <dt>{{ t('common.address') }}</dt>
        <dd>
          <code>{{ endpoint.host }}</code>
        </dd>
        <dt>{{ t('common.port') }}</dt>
        <dd class="num">{{ endpoint.port }}</dd>
        <dt>{{ t('common.user') }}</dt>
        <dd>{{ user }}</dd>
      </dl>

      <section v-if="mode === 'changed'" class="key key--old" :aria-label="t('trust.knownKeys')">
        <h3>
          <ShieldCheck :size="16" :stroke-width="1.75" aria-hidden="true" /> {{ t('trust.known') }}
        </h3>
        <div v-for="k in known" :key="k.fingerprint" class="key-row">
          <span class="alg">{{ k.algorithm }}</span>
          <code class="fp">{{ k.fingerprint }}</code>
        </div>
      </section>
      <section
        class="key"
        :class="{ 'key--new': mode === 'changed' }"
        :aria-label="t('trust.presentedKeys')"
      >
        <h3 v-if="mode === 'changed'">
          <ShieldAlert :size="16" :stroke-width="1.75" aria-hidden="true" />
          {{ t('trust.presented') }}
        </h3>
        <div class="key-row">
          <span class="alg">{{ presented.algorithm }}</span>
          <code class="fp">{{ presented.fingerprint }}</code>
        </div>
      </section>
      <p class="muted">{{ t('trust.scope') }}</p>
    </div>

    <template #footer>
      <template v-if="mode === 'first'">
        <UiButton autofocus :disabled="busy" @click="emit('cancel')">
          {{ t('trust.cancelConnect') }}
        </UiButton>
        <UiButton variant="primary" :busy="busy" @click="emit('trust')">{{
          t('trust.trustAndConnect')
        }}</UiButton>
      </template>
      <template v-else>
        <UiButton autofocus @click="emit('cancel')">{{ t('common.close') }}</UiButton>
        <UiButton variant="tonal" @click="emit('manage')">
          {{ t('trust.manage') }}
        </UiButton>
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

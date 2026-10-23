<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'

import UiButton from '@/components/ui/UiButton.vue'
import UiCheckbox from '@/components/ui/UiCheckbox.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { useT } from '@/i18n'
import type { CredentialPrompt } from '@/ipc'

import { credentialTitle } from './credentialLabels'

const t = useT()

// D04 (IXD-31, IXD-32, SEC-02, SEC-03). The title names what the secret is for; the input
// starts empty and masked; "remember" starts unticked unless it replaces a remembered value
// that failed. The typed value is cleared as soon as it is handed over.
const props = defineProps<{
  prompt: CredentialPrompt | null
  /** Changes for every new request, including each retry. */
  requestId: number | null
  busy?: boolean
}>()
const emit = defineEmits<{ submit: [secret: string, remember: boolean]; cancel: [] }>()

const secret = ref('')
const remember = ref(false)
const form = ref<HTMLFormElement | null>(null)

watch(
  () => props.requestId,
  async () => {
    secret.value = ''
    remember.value = props.prompt?.rememberDefault ?? false
    await nextTick()
    form.value
      ?.querySelector<HTMLInputElement>('input[type="password"],input[type="text"]')
      ?.focus()
  },
)

const remaining = computed(() =>
  props.prompt ? props.prompt.maxAttempts - props.prompt.attempt + 1 : 0,
)

const fieldLabel = computed(() => {
  switch (props.prompt?.purpose) {
    case 'keyPassphrase':
      return t('credential.field.keyPassphrase')
    case 'sudoPassword':
      return t('credential.field.sudoPassword')
    default:
      return t('credential.field.password')
  }
})

function submit() {
  if (!secret.value || props.busy) return
  const value = secret.value
  secret.value = ''
  emit('submit', value, remember.value)
}
</script>

<template>
  <UiDialog
    :open="!!prompt"
    :title="prompt ? credentialTitle(prompt) : ''"
    width="credential"
    :cancellable="!busy"
    @cancel="emit('cancel')"
  >
    <form v-if="prompt" ref="form" class="credential" @submit.prevent="submit">
      <UiNotice v-if="prompt.savedRejected" tone="warning">
        {{ t('credential.savedRejected') }}
      </UiNotice>
      <dl class="facts">
        <dt>{{ t('common.server') }}</dt>
        <dd>
          {{ prompt.serverName }} <code>{{ prompt.address }}</code>
        </dd>
        <dt>{{ t('credential.user') }}</dt>
        <dd>{{ prompt.user }}</dd>
        <template v-if="prompt.purpose === 'sudoPassword'">
          <dt>{{ t('credential.runAs') }}</dt>
          <dd>{{ prompt.runAs }}</dd>
        </template>
        <template v-if="prompt.keyPath">
          <dt>{{ t('credential.keyFile') }}</dt>
          <dd>
            <code class="path">{{ prompt.keyPath }}</code>
          </dd>
        </template>
        <dt>{{ t('credential.usedFor') }}</dt>
        <dd>{{ prompt.context }}</dd>
      </dl>

      <div class="field">
        <UiTextInput
          :key="requestId ?? 0"
          v-model="secret"
          type="password"
          :aria-label="fieldLabel"
          :placeholder="fieldLabel"
          :invalid="prompt.rejected && !prompt.savedRejected"
          autofocus
        />
        <p v-if="prompt.rejected && !prompt.savedRejected" class="error" role="alert">
          {{ t('credential.remaining', { n: remaining }) }}
        </p>
      </div>

      <div class="remember">
        <UiCheckbox v-model="remember" :label="t('credential.remember')" />
        <p v-if="remember" class="hint">{{ t('credential.rememberHint') }}</p>
      </div>
      <button type="submit" hidden />
    </form>

    <template #footer>
      <UiButton :disabled="busy" @click="emit('cancel')">{{ t('common.cancel') }}</UiButton>
      <UiButton
        variant="primary"
        :busy="busy"
        :disabled-reason="secret ? null : t('credential.enter', { field: fieldLabel })"
        @click="submit"
      >
        {{ t('common.ok') }}
      </UiButton>
    </template>
  </UiDialog>
</template>

<style scoped>
.credential {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
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
  overflow-wrap: anywhere;
}

.path {
  font: var(--font-code);
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.error {
  margin: 0;
  color: var(--state-failure);
  font: var(--font-caption);
}

.remember {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.hint {
  margin: 0 0 0 26px;
  color: var(--text-secondary);
  font: var(--font-caption);
}
</style>

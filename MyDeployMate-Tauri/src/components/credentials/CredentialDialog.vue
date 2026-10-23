<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'

import UiButton from '@/components/ui/UiButton.vue'
import UiCheckbox from '@/components/ui/UiCheckbox.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import type { CredentialPrompt } from '@/ipc'

import { credentialTitle } from './credentialLabels'

// D04 (IXD-31, IXD-32, SEC-02, SEC-03). The title names what the secret is for; the input
// starts empty and masked; "remember" starts unticked unless it replaces a remembered value
// that failed. The typed value is cleared as soon as it is handed over.
const props = defineProps<{ prompt: CredentialPrompt | null; busy?: boolean }>()
const emit = defineEmits<{ submit: [secret: string, remember: boolean]; cancel: [] }>()

const secret = ref('')
const remember = ref(false)
const form = ref<HTMLFormElement | null>(null)

watch(
  () => props.prompt?.requestId,
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
      return '私钥口令'
    case 'sudoPassword':
      return 'sudo 密码'
    default:
      return '密码'
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
        已保存的凭据未通过验证，请重新输入。
      </UiNotice>
      <dl class="facts">
        <dt>服务器</dt>
        <dd>
          {{ prompt.serverName }} <code>{{ prompt.address }}</code>
        </dd>
        <dt>登录账号</dt>
        <dd>{{ prompt.user }}</dd>
        <template v-if="prompt.purpose === 'sudoPassword'">
          <dt>提权为</dt>
          <dd>{{ prompt.runAs }}</dd>
        </template>
        <template v-if="prompt.keyPath">
          <dt>私钥文件</dt>
          <dd>
            <code class="path">{{ prompt.keyPath }}</code>
          </dd>
        </template>
        <dt>用于</dt>
        <dd>{{ prompt.context }}</dd>
      </dl>

      <div class="field">
        <UiTextInput
          :key="prompt.requestId"
          v-model="secret"
          type="password"
          :aria-label="fieldLabel"
          :placeholder="fieldLabel"
          :invalid="prompt.rejected && !prompt.savedRejected"
          autofocus
        />
        <p v-if="prompt.rejected && !prompt.savedRejected" class="error" role="alert">
          验证未通过，剩余 {{ remaining }} 次
        </p>
      </div>

      <div class="remember">
        <UiCheckbox v-model="remember" label="记住此凭据（保存到 Windows 凭据管理器）" />
        <p v-if="remember" class="hint">可在配置中心 › 已保存凭据中清除</p>
      </div>
      <button type="submit" hidden />
    </form>

    <template #footer>
      <UiButton :disabled="busy" @click="emit('cancel')">取消</UiButton>
      <UiButton
        variant="primary"
        :busy="busy"
        :disabled-reason="secret ? null : `请输入${fieldLabel}`"
        @click="submit"
      >
        确定
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

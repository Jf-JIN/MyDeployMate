<script setup lang="ts">
import { KeyRound } from '@lucide/vue'
import { ref, watch } from 'vue'

import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiTag from '@/components/ui/UiTag.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { call, toAppError } from '@/ipc'

// The value of a sensitive variable kept in Windows Credential Manager (VAR-03, ARC-10). It is
// written at once when entered, never into the workspace draft, and never read back into the
// page: the field only shows whether a value exists.
const props = defineProps<{
  variableId: string
  /** Set for an environment override, which has its own value. */
  environmentId?: string | null
  label: string
}>()
const emit = defineEmits<{ saved: [] }>()

const present = ref<boolean | null>(null)
const error = ref<string | null>(null)

async function check() {
  try {
    present.value = await call('variable_secret_present', {
      variableId: props.variableId,
      environmentId: props.environmentId ?? null,
    })
  } catch (e) {
    present.value = null
    error.value = toAppError(e).message
  }
}
watch(() => [props.variableId, props.environmentId], check, { immediate: true })

const open = ref(false)
const value = ref('')
const busy = ref(false)
const saveError = ref<string | null>(null)

function start() {
  value.value = ''
  saveError.value = null
  open.value = true
}

async function save() {
  if (!value.value) return
  busy.value = true
  const typed = value.value
  value.value = ''
  try {
    await call('set_variable_secret', {
      variableId: props.variableId,
      environmentId: props.environmentId ?? null,
      value: typed,
    })
    open.value = false
    present.value = true
    emit('saved')
  } catch (e) {
    saveError.value = toAppError(e).message
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <span class="secret-field">
    <UiTag v-if="present" tone="success" :icon="KeyRound">已保存在凭据管理器</UiTag>
    <UiTag v-else-if="present === false" tone="warning">需填写</UiTag>
    <UiTag v-else-if="error" tone="failure" :title="error">凭据管理器不可用</UiTag>
    <UiButton compact variant="ghost" :disabled-reason="error" @click="start">
      {{ present ? '更改' : '填写' }}
    </UiButton>
  </span>

  <UiDialog
    :open="open"
    :title="`填写 ${label}`"
    width="credential"
    :cancellable="!busy"
    @cancel="open = false"
  >
    <form class="secret-form" @submit.prevent="save">
      <p class="hint">值保存到 Windows 凭据管理器，不写入配置文件，也不会显示在界面上。</p>
      <UiTextInput v-model="value" type="password" :aria-label="label" autofocus />
      <UiNotice v-if="saveError" tone="failure">{{ saveError }}</UiNotice>
      <button type="submit" hidden />
    </form>
    <template #footer>
      <UiButton :disabled="busy" @click="open = false">取消</UiButton>
      <UiButton
        variant="primary"
        :busy="busy"
        :disabled-reason="value ? null : '请输入值'"
        @click="save"
      >
        保存
      </UiButton>
    </template>
  </UiDialog>
</template>

<style scoped>
.secret-field {
  display: inline-flex;
  flex: 1;
  align-items: center;
  gap: var(--space-2);
}

.secret-form {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.hint {
  margin: 0;
  color: var(--text-secondary);
}
</style>

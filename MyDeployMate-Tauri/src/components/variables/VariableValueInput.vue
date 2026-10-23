<script setup lang="ts">
import { computed } from 'vue'

import UiSelect from '@/components/ui/UiSelect.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import type { VariableValue } from '@/ipc'

import SecretValueField from './SecretValueField.vue'

// A variable value with its source (VAR-01, IXD-13): typed in, read from an environment
// variable, or kept in Windows Credential Manager. Sensitive values cannot be typed in.
const model = defineModel<VariableValue>({ required: true })
const props = defineProps<{
  label: string
  sensitive: boolean
  error?: string | null
  /** Where a Credential Manager value is kept: the variable, and the environment for an override. */
  variableId: string
  environmentId?: string | null
}>()
const emit = defineEmits<{ 'secret-saved': [] }>()

type Kind = VariableValue['kind']

const options = computed(() => [
  {
    value: 'literal' as Kind,
    label: '直接填写',
    disabledReason: props.sensitive ? '敏感变量不能以明文保存' : undefined,
  },
  { value: 'envRef' as Kind, label: '环境变量' },
  { value: 'secretRef' as Kind, label: '凭据管理器' },
])

const kind = computed<Kind>({
  get: () => model.value.kind,
  set(next) {
    if (next === model.value.kind) return
    model.value =
      next === 'literal'
        ? { kind: next, value: '' }
        : next === 'envRef'
          ? { kind: next, name: '' }
          : { kind: 'secretRef' }
  },
})

const text = computed({
  get: () =>
    model.value.kind === 'literal'
      ? model.value.value
      : model.value.kind === 'envRef'
        ? model.value.name
        : '',
  set(value: string) {
    if (model.value.kind === 'literal') model.value = { kind: 'literal', value }
    else if (model.value.kind === 'envRef') model.value = { kind: 'envRef', name: value }
  },
})
</script>

<template>
  <div class="value-input">
    <UiSelect v-model="kind" class="kind" :options="options" :aria-label="`${label}的来源`" />
    <UiTextInput
      v-if="kind !== 'secretRef'"
      v-model="text"
      class="text"
      mono
      :placeholder="kind === 'envRef' ? '环境变量名' : undefined"
      :invalid="!!error"
      :aria-label="kind === 'envRef' ? `${label}读取的环境变量名` : label"
    />
    <SecretValueField
      v-else
      :variable-id="variableId"
      :environment-id="environmentId"
      :label="label"
      @saved="emit('secret-saved')"
    />
  </div>
  <p v-if="error" class="error">{{ error }}</p>
</template>

<style scoped>
.value-input {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.kind {
  flex: none;
  width: 116px;
}

.text {
  flex: 1;
  min-width: 0;
}

.error {
  margin: var(--space-1) 0 0;
  color: var(--state-failure);
  font: var(--font-caption);
}
</style>

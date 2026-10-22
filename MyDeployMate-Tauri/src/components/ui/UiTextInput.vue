<script setup lang="ts">
import { Eye, EyeOff } from '@lucide/vue'
import { computed, ref } from 'vue'

// UI-16 / UI-17: 36 px input; readonly stays readable and copyable; password masking with a
// fixed-position reveal toggle (UI-27).
const model = defineModel<string>({ required: true })

const props = withDefaults(
  defineProps<{
    id?: string
    type?: 'text' | 'password'
    invalid?: boolean
    readonly?: boolean
    disabled?: boolean
    mono?: boolean
    placeholder?: string
    describedBy?: string
    autofocus?: boolean
  }>(),
  { type: 'text', id: undefined, placeholder: undefined, describedBy: undefined },
)

const revealed = ref(false)
const inputType = computed(() =>
  props.type === 'password' && !revealed.value ? 'password' : 'text',
)
</script>

<template>
  <div class="text-input" :class="{ 'text-input--password': type === 'password' }">
    <input
      :id="id"
      v-model="model"
      class="control"
      :class="{ 'control--mono': mono }"
      :type="inputType"
      :readonly="readonly"
      :disabled="disabled"
      :placeholder="placeholder"
      :aria-invalid="invalid || undefined"
      :aria-describedby="describedBy"
      :autofocus="autofocus"
      :autocomplete="type === 'password' ? 'off' : undefined"
      spellcheck="false"
    />
    <button
      v-if="type === 'password'"
      type="button"
      class="reveal"
      :aria-label="revealed ? '隐藏内容' : '显示内容'"
      :title="revealed ? '隐藏内容' : '显示内容'"
      :aria-pressed="revealed"
      :disabled="disabled"
      @click="revealed = !revealed"
    >
      <component :is="revealed ? EyeOff : Eye" :size="16" :stroke-width="1.75" aria-hidden="true" />
    </button>
  </div>
</template>

<style scoped>
.text-input {
  position: relative;
  min-width: 0;
}

.text-input--password .control {
  padding-right: 40px;
}

.reveal {
  position: absolute;
  top: 2px;
  right: 2px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  padding: 0;
  border: 0;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}

.reveal:hover:not(:disabled) {
  background: var(--surface-hover);
}
</style>

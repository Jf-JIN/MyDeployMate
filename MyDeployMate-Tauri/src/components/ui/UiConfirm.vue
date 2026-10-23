<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'

import { useT } from '@/i18n'

import UiButton from './UiButton.vue'
import UiDialog from './UiDialog.vue'
import UiTextInput from './UiTextInput.vue'

const t = useT()

// Confirmation layer (UI-14, UI-28, IXD-03). The confirm button is never the default for Enter;
// with `requireText` the user must type the name shown next to the field (protected
// environments, RUN-02 / SRS-7).
const props = withDefaults(
  defineProps<{
    open: boolean
    title: string
    confirmLabel: string
    cancelLabel?: string | null
    danger?: boolean
    requireText?: string | null
    /** Why the action cannot be confirmed now; the confirm button stays disabled. */
    blockedReason?: string | null
    busy?: boolean
  }>(),
  { cancelLabel: null, requireText: null, blockedReason: null },
)

const emit = defineEmits<{ confirm: []; cancel: [] }>()

const typed = ref('')
const inputId = useId()
watch(
  () => props.open,
  () => (typed.value = ''),
)

const reason = computed(
  () =>
    props.blockedReason ??
    (props.requireText && typed.value !== props.requireText
      ? t('ui.confirm.typeToContinue', { text: props.requireText })
      : null),
)
</script>

<template>
  <UiDialog
    :open="open"
    :title="title"
    width="credential"
    :cancellable="!busy"
    @cancel="emit('cancel')"
  >
    <div class="confirm-body">
      <slot />
      <div v-if="requireText" class="confirm-type">
        <i18n-t
          keypath="ui.confirm.typeLabel"
          tag="label"
          :for="inputId"
          class="confirm-type-label"
        >
          <template #text>
            <strong class="confirm-type-name">{{ requireText }}</strong>
          </template>
        </i18n-t>
        <UiTextInput :id="inputId" v-model="typed" mono autofocus />
      </div>
    </div>
    <template #footer>
      <UiButton :disabled="busy" @click="emit('cancel')">
        {{ cancelLabel ?? t('common.cancel') }}
      </UiButton>
      <UiButton
        :variant="danger ? 'danger' : 'primary'"
        :busy="busy"
        :disabled-reason="reason"
        @click="emit('confirm')"
      >
        {{ confirmLabel }}
      </UiButton>
    </template>
  </UiDialog>
</template>

<style scoped>
.confirm-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  overflow-wrap: anywhere;
}

.confirm-type {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.confirm-type-name {
  font: var(--font-code);
  font-weight: 600;
}
</style>

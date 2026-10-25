<script setup lang="ts">
import { CornerDownLeft, SquareStop } from '@lucide/vue'
import { ref, watch } from 'vue'

import UiButton from '@/components/ui/UiButton.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { useT } from '@/i18n'
import { errorText } from '@/i18n/message'
import type { RunId } from '@/ipc'
import { useRunStore } from '@/stores/run'
import { useToastStore } from '@/stores/toast'

const t = useT()

// RUN-08: input for the running command, one line at a time. There is no terminal: programs
// that read lines (Python input(), shell read) work, full-screen programs do not. A hidden
// line is masked in the log and the run's records.
const props = defineProps<{ runId: RunId; seq: number }>()

const runs = useRunStore()
const toast = useToastStore()
const line = ref('')
const hidden = ref(false)
const busy = ref(false)
const closed = ref(false)

watch(
  () => props.seq,
  () => {
    line.value = ''
    closed.value = false
  },
)

async function send(text: string | null) {
  if (busy.value) return
  busy.value = true
  try {
    await runs.sendInput(props.runId, props.seq, text, hidden.value)
    if (text === null) closed.value = true
    else line.value = ''
  } catch (e) {
    toast.show(errorText(e))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <form
    class="input-bar"
    :aria-label="t('runInput.label', { n: seq + 1 })"
    @submit.prevent="send(line)"
  >
    <UiTextInput
      v-model="line"
      class="line"
      mono
      :type="hidden ? 'password' : 'text'"
      :disabled="closed"
      :placeholder="closed ? t('runInput.closed') : t('runInput.placeholder', { n: seq + 1 })"
      :aria-label="t('runInput.label', { n: seq + 1 })"
    />
    <label class="hide">
      <UiSwitch v-model="hidden" :label="t('runInput.hidden')" :disabled="closed" />
      <span>{{ t('runInput.hidden') }}</span>
    </label>
    <UiButton
      type="submit"
      variant="primary"
      :icon="CornerDownLeft"
      :busy="busy"
      :disabled="closed"
      >{{ t('runInput.send') }}</UiButton
    >
    <UiButton
      :icon="SquareStop"
      :disabled="closed || busy"
      :title="t('runInput.closeHint')"
      @click="send(null)"
      >{{ t('runInput.close') }}</UiButton
    >
  </form>
</template>

<style scoped>
.input-bar {
  display: flex;
  flex: none;
  align-items: center;
  gap: var(--space-2);
}

.line {
  flex: 1;
}

.hide {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--text-secondary);
  font: var(--font-caption);
  white-space: nowrap;
}
</style>

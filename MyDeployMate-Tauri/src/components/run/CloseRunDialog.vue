<script setup lang="ts">
import { listen } from '@tauri-apps/api/event'
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import UiConfirm from '@/components/ui/UiConfirm.vue'
import { useT } from '@/i18n'
import { call, toAppError } from '@/ipc'
import { useToastStore } from '@/stores/toast'

const t = useT()

// IXD-29: closing the window during a run asks first. "取消并退出" cancels the run and the app
// quits once it has stopped; nothing claims the run stopped before then.
const router = useRouter()
const toast = useToastStore()
const open = ref(false)
const quitting = ref(false)

let unlisten: (() => void) | null = null
onMounted(async () => {
  try {
    unlisten = await listen<string>('run://close-requested', (event) => {
      if (quitting.value) return
      open.value = true
      void router.push({ name: 'run', params: { runId: event.payload } })
    })
  } catch {
    // Outside the desktop shell there is no window to close.
  }
})
onBeforeUnmount(() => unlisten?.())

async function quit() {
  quitting.value = true
  try {
    await call('cancel_and_quit')
  } catch (e) {
    quitting.value = false
    toast.show(toAppError(e).message)
  }
}
</script>

<template>
  <UiConfirm
    :open="open"
    :title="quitting ? t('closeRun.stoppingTitle') : t('closeRun.activeTitle')"
    :confirm-label="t('closeRun.confirm')"
    :cancel-label="t('closeRun.back')"
    danger
    :busy="quitting"
    @confirm="quit"
    @cancel="open = false"
  >
    <p v-if="quitting" class="close-text">{{ t('closeRun.stopping') }}</p>
    <p v-else class="close-text">
      {{ t('closeRun.body') }}
    </p>
  </UiConfirm>
</template>

<style scoped>
.close-text {
  margin: 0;
}
</style>

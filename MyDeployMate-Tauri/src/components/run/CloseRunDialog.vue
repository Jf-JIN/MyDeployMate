<script setup lang="ts">
import { listen } from '@tauri-apps/api/event'
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import UiConfirm from '@/components/ui/UiConfirm.vue'
import { call, toAppError } from '@/ipc'
import { useToastStore } from '@/stores/toast'

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
    :title="quitting ? '正在停止运行' : '有运行正在进行'"
    confirm-label="取消并退出"
    cancel-label="返回运行"
    danger
    :busy="quitting"
    @confirm="quit"
    @cancel="open = false"
  >
    <p v-if="quitting" class="close-text">正在停止当前任务，停止后应用会自动退出。</p>
    <p v-else class="close-text">
      关闭应用前需要先取消运行。取消后不再开始后续任务，已完成的操作不会自动撤销。
    </p>
  </UiConfirm>
</template>

<style scoped>
.close-text {
  margin: 0;
}
</style>

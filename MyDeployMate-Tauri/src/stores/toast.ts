import { defineStore } from 'pinia'
import { ref } from 'vue'

/** Seconds a plain success message stays (UI-19). */
export const TOAST_MS = 3000

export interface Toast {
  id: number
  message: string
}

/**
 * Short-lived confirmations such as "已保存". Anything the user must act on (failures, permission
 * denials, identity changes, unknown results) belongs in an inline UiNotice instead.
 */
export const useToastStore = defineStore('toast', () => {
  const toasts = ref<Toast[]>([])
  let next = 1

  function dismiss(id: number) {
    toasts.value = toasts.value.filter((t) => t.id !== id)
  }

  function show(message: string) {
    const id = next++
    toasts.value = [...toasts.value, { id, message }]
    setTimeout(() => dismiss(id), TOAST_MS)
    return id
  }

  return { toasts, show, dismiss }
})

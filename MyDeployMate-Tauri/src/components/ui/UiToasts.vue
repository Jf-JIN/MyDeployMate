<script setup lang="ts">
import { CircleCheck } from '@lucide/vue'

import { useToastStore } from '@/stores/toast'

const store = useToastStore()
</script>

<template>
  <div class="toasts" role="status" aria-live="polite">
    <div v-for="toast in store.toasts" :key="toast.id" class="toast">
      <CircleCheck :size="16" :stroke-width="2" aria-hidden="true" />
      <span>{{ toast.message }}</span>
    </div>
  </div>
</template>

<style scoped>
.toasts {
  position: fixed;
  right: var(--space-5);
  bottom: var(--space-5);
  z-index: 200;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  pointer-events: none;
}

.toast {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-control);
  background: var(--surface-panel);
  box-shadow: 0 4px 16px rgb(32 36 40 / 12%);
  color: var(--state-success);
}

.toast span {
  color: var(--text-primary);
}
</style>

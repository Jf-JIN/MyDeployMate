<script setup lang="ts">
import UiNotice from '@/components/ui/UiNotice.vue'
import type { AppError, Issue } from '@/ipc'

// Save failures a page cannot show next to a field: storage errors and issues elsewhere in the
// configuration that still block saving (UI-19).
defineProps<{ error: AppError | null; issues: Issue[] }>()
</script>

<template>
  <UiNotice v-if="error" tone="failure">{{ error.message }}</UiNotice>
  <UiNotice v-if="issues.length" tone="warning">
    配置的其他位置还有 {{ issues.length }} 个问题，需要先修正才能保存：
    <ul class="issues">
      <li v-for="issue in issues" :key="issue.path">{{ issue.message }}</li>
    </ul>
  </UiNotice>
</template>

<style scoped>
.issues {
  margin: var(--space-1) 0 0;
  padding-left: var(--space-5);
}
</style>

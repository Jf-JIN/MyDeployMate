<script setup lang="ts">
import UiNotice from '@/components/ui/UiNotice.vue'
import { useT } from '@/i18n'
import type { AppError, Issue } from '@/ipc'

const t = useT()

// Save failures a page cannot show next to a field: storage errors and issues elsewhere in the
// configuration that still block saving (UI-19).
defineProps<{ error: AppError | null; issues: Issue[] }>()
</script>

<template>
  <UiNotice v-if="error" tone="failure">{{ error.message }}</UiNotice>
  <UiNotice v-if="issues.length" tone="warning">
    {{ t('drafts.issuesElsewhere', { n: issues.length }) }}
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

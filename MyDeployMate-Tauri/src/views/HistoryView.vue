<script setup lang="ts">
import { History } from '@lucide/vue'

import EmptyState from '@/components/EmptyState.vue'
import PageLayout from '@/components/PageLayout.vue'
import { useT } from '@/i18n'

const t = useT()
</script>

<template>
  <PageLayout :title="t('history.title')">
    <EmptyState
      :title="t('history.emptyTitle')"
      :detail="t('history.emptyDetail')"
      :icon="History"
    />
  </PageLayout>
</template>

<script setup lang="ts">
import { LayoutTemplate } from '@lucide/vue'

import EmptyState from '@/components/EmptyState.vue'
import PageLayout from '@/components/PageLayout.vue'
import { useT } from '@/i18n'

const t = useT()
</script>

<template>
  <PageLayout :title="t('templates.title')">
    <EmptyState :title="t('templates.empty')" :icon="LayoutTemplate" />
  </PageLayout>
</template>

<script setup lang="ts">
import { LayoutTemplate } from '@lucide/vue'

import EmptyState from '@/components/EmptyState.vue'
import PageLayout from '@/components/PageLayout.vue'
</script>

<template>
  <PageLayout title="流程模板">
    <EmptyState title="模板将在后续版本提供" :icon="LayoutTemplate" />
  </PageLayout>
</template>

<script setup lang="ts">
import { History } from '@lucide/vue'

import EmptyState from '@/components/EmptyState.vue'
import PageLayout from '@/components/PageLayout.vue'
</script>

<template>
  <PageLayout title="执行记录">
    <EmptyState
      title="尚无执行记录"
      detail="执行流程后，每次运行的结果和日志会列在这里。"
      :icon="History"
    />
  </PageLayout>
</template>

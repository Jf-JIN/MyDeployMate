<script setup lang="ts">
import EmptyState from '@/components/EmptyState.vue'
import type { Pipeline, Project } from '@/ipc'

// P03 target binding; filled in by task 3.6.
defineProps<{ project: Project; pipeline: Pipeline | null }>()
</script>

<template>
  <EmptyState title="目标绑定将在阶段 3.6 提供" />
</template>

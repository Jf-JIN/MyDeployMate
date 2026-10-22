<script setup lang="ts">
import EmptyState from '@/components/EmptyState.vue'
import type { Pipeline, Project } from '@/ipc'

// P02 pipeline editor; filled in by task 3.5.
defineProps<{ project: Project; pipeline: Pipeline | null }>()
</script>

<template>
  <EmptyState title="流程编辑将在阶段 3.5 提供" />
</template>

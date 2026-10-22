<script setup lang="ts">
import EmptyState from '@/components/EmptyState.vue'
import type { Pipeline, Project } from '@/ipc'

// P04 variables and environment overrides; filled in by task 3.6.
defineProps<{ project: Project; pipeline: Pipeline | null }>()
</script>

<template>
  <EmptyState title="变量与环境覆盖将在阶段 3.6 提供" />
</template>

<script setup lang="ts">
import EmptyState from '@/components/EmptyState.vue'
import type { Pipeline, Project } from '@/ipc'

// P05 project settings; filled in by task 3.4.
defineProps<{ project: Project; pipeline: Pipeline | null }>()
</script>

<template>
  <EmptyState title="项目设置将在阶段 3.4 提供" />
</template>

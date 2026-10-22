<script setup lang="ts">
import EmptyState from '@/components/EmptyState.vue'
import type { Pipeline, Project } from '@/ipc'

// A project without pipelines; creating one arrives with task 3.5.
defineProps<{ project: Project; pipeline: Pipeline | null }>()
</script>

<template>
  <EmptyState title="尚无流程" />
</template>

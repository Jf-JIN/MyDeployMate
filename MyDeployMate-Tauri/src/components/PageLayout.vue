<script setup lang="ts">
import ActiveRunChip from '@/components/ActiveRunChip.vue'

// Pages outside a project: a 56 px title bar (with the active-run entry, IXD-02) and a
// scrolling body.
defineProps<{ title: string }>()
</script>

<template>
  <div class="page">
    <header class="page-bar">
      <h1 class="page-title">{{ title }}</h1>
      <div class="page-actions">
        <ActiveRunChip />
        <slot name="actions" />
      </div>
    </header>
    <div class="page-body">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.page-bar {
  display: flex;
  flex: none;
  align-items: center;
  gap: var(--space-3);
  height: var(--context-bar-height);
  padding: 0 var(--space-5);
  border-bottom: 1px solid var(--border-subtle);
  background: var(--surface-panel);
}

.page-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.page-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.page-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: var(--space-4) var(--space-5);
}
</style>

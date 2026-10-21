<script setup lang="ts">
defineProps<{ title: string; detail?: string }>()
</script>

<template>
  <div class="empty">
    <p class="title">{{ title }}</p>
    <p v-if="detail" class="detail">{{ detail }}</p>
    <slot />
  </div>
</template>

<style scoped>
.empty {
  padding: var(--space-6) 0;
  color: var(--text-secondary);
}

.title {
  margin: 0;
  color: var(--text-primary);
}

.detail {
  margin: var(--space-1) 0 0;
  font: var(--font-caption);
}
</style>

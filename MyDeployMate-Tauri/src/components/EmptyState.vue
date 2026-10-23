<script setup lang="ts">
import type { Component } from 'vue'

// Empty pages and empty results (UI-02): a line icon, what is missing, why it matters and the
// next action. `compact` is for empty results inside a page, such as a filtered table.
defineProps<{ title: string; detail?: string; icon?: Component; compact?: boolean }>()
</script>

<template>
  <div class="empty" :class="{ 'empty--compact': compact }">
    <span v-if="icon" class="icon" aria-hidden="true">
      <component :is="icon" :size="compact ? 20 : 28" :stroke-width="1.5" />
    </span>
    <p class="title">{{ title }}</p>
    <p v-if="detail" class="detail">{{ detail }}</p>
    <div v-if="$slots.default" class="actions"><slot /></div>
    <slot name="after" />
  </div>
</template>

<style scoped>
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-2);
  max-width: 520px;
  margin: 0 auto;
  padding: 64px var(--space-5);
  color: var(--text-secondary);
  text-align: center;
}

.empty--compact {
  gap: var(--space-1);
  padding: var(--space-6) var(--space-4);
}

.icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 56px;
  height: 56px;
  margin-bottom: var(--space-2);
  border-radius: 50%;
  background: var(--surface-selected);
  color: var(--action-primary);
}

.empty--compact .icon {
  width: 40px;
  height: 40px;
  margin-bottom: var(--space-1);
}

.title {
  margin: 0;
  color: var(--text-primary);
  font: var(--font-section);
}

.empty--compact .title {
  font: var(--font-label);
}

.detail {
  margin: 0;
  font: var(--font-body);
}

.empty--compact .detail {
  font: var(--font-caption);
}

.actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: var(--space-2);
  margin-top: var(--space-3);
}

.empty--compact .actions {
  margin-top: var(--space-2);
}
</style>

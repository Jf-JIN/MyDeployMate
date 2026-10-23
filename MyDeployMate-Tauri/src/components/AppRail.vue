<script setup lang="ts">
import { FolderKanban, History, Layers, LayoutTemplate, Server, Settings } from '@lucide/vue'
import { computed } from 'vue'

import { useT } from '@/i18n'

const t = useT()

// UI-10 (v0.3): first-level navigation as a 56 px icon rail; every item has a tooltip and an
// accessible name, and the current item gets the selected background plus a 3 px edge.
const main = computed(() => [
  { to: '/projects', label: t('nav.projects'), icon: FolderKanban },
  { to: '/servers', label: t('nav.servers'), icon: Server },
  { to: '/history', label: t('nav.history'), icon: History },
  { to: '/templates', label: t('nav.templates'), icon: LayoutTemplate },
])
const bottom = computed(() => [{ to: '/settings', label: t('nav.settings'), icon: Settings }])
</script>

<template>
  <nav class="rail" :aria-label="t('nav.main')">
    <span class="brand" title="MyDeployMate" aria-hidden="true">
      <Layers :size="16" :stroke-width="2" />
    </span>
    <RouterLink
      v-for="item in main"
      :key="item.to"
      :to="item.to"
      class="rail-item"
      :aria-label="item.label"
      :title="item.label"
    >
      <component :is="item.icon" :size="20" :stroke-width="1.75" aria-hidden="true" />
    </RouterLink>
    <div class="rail-bottom">
      <slot name="bottom" />
      <RouterLink
        v-for="item in bottom"
        :key="item.to"
        :to="item.to"
        class="rail-item"
        :aria-label="item.label"
        :title="item.label"
      >
        <component :is="item.icon" :size="20" :stroke-width="1.75" aria-hidden="true" />
      </RouterLink>
    </div>
  </nav>
</template>

<style scoped>
.rail {
  display: flex;
  flex: none;
  flex-direction: column;
  align-items: center;
  gap: var(--space-1);
  width: var(--rail-width);
  padding: var(--space-3) 0;
  border-right: 1px solid var(--border-subtle);
  background: var(--surface-sidebar);
}

.brand {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  margin-bottom: var(--space-3);
  border-radius: var(--radius-control);
  background: var(--action-primary);
  color: var(--text-on-action);
}

.rail-item {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: var(--rail-item-size);
  height: var(--rail-item-size);
  border-radius: var(--radius-control);
  color: var(--text-secondary);
  transition: background-color var(--motion-fast);
}

.rail-item:hover {
  background: var(--surface-sidebar-hover);
  color: var(--text-primary);
}

.rail-item.router-link-active {
  background: var(--surface-selected);
  box-shadow: inset 3px 0 0 var(--action-primary);
  color: var(--action-primary);
}

.rail-bottom {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-1);
  margin-top: auto;
}
</style>

<script setup lang="ts">
import { FolderKanban, History, LayoutTemplate, Server, Settings } from '@lucide/vue'

const items = [
  { to: '/projects', label: '项目与流程', icon: FolderKanban },
  { to: '/servers', label: '服务器', icon: Server },
  { to: '/history', label: '执行记录', icon: History },
  { to: '/templates', label: '流程模板', icon: LayoutTemplate },
  { to: '/settings', label: '配置中心', icon: Settings },
]
</script>

<template>
  <nav class="sidebar" aria-label="主导航">
    <div class="brand">MyDeployMate</div>
    <RouterLink v-for="item in items" :key="item.to" :to="item.to" class="nav-item">
      <component :is="item.icon" :size="18" :stroke-width="1.75" aria-hidden="true" />
      <span>{{ item.label }}</span>
    </RouterLink>
  </nav>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  width: var(--sidebar-width);
  flex-shrink: 0;
  padding: var(--space-3) var(--space-2);
  background: var(--surface-panel);
  border-right: 1px solid var(--border-subtle);
}

.brand {
  font: var(--font-section);
  padding: var(--space-2) var(--space-3) var(--space-4);
}

.nav-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  height: var(--menu-item-height);
  padding: 0 var(--space-3);
  border-radius: var(--radius-control);
  color: var(--text-primary);
  text-decoration: none;
}

.nav-item:hover {
  background: var(--surface-hover);
}

.nav-item.router-link-active {
  background: var(--surface-selected);
  box-shadow: inset 3px 0 0 var(--action-primary);
  font-weight: 600;
}
</style>

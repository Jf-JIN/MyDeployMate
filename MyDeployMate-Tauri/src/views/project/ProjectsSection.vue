<script setup lang="ts">
import { PanelLeftOpen } from '@lucide/vue'

import ProjectTree from '@/components/ProjectTree.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import { useSessionStore } from '@/stores/session'

// "项目与流程" section: the project tree (collapsible) beside the routed page.
const session = useSessionStore()
</script>

<template>
  <div class="section">
    <ProjectTree v-if="!session.treeCollapsed" />
    <div v-else class="tree-collapsed">
      <UiIconButton
        :icon="PanelLeftOpen"
        label="展开项目树"
        @click="session.treeCollapsed = false"
      />
    </div>
    <div class="section-page">
      <RouterView />
    </div>
  </div>
</template>

<style scoped>
.section {
  display: flex;
  flex: 1;
  min-width: 0;
  min-height: 0;
}

.tree-collapsed {
  display: flex;
  flex: none;
  justify-content: center;
  width: 40px;
  padding-top: var(--space-3);
  border-right: 1px solid var(--border-subtle);
  background: var(--surface-panel);
}

.section-page {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}
</style>

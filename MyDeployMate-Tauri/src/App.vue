<script setup lang="ts">
import { onMounted } from 'vue'

import AppSidebar from './components/AppSidebar.vue'
import { useWorkspaceStore } from './stores/workspace'

const workspace = useWorkspaceStore()
onMounted(() => workspace.load())
</script>

<template>
  <div class="shell">
    <AppSidebar />
    <main class="content">
      <p v-if="workspace.error" class="banner banner--failure" role="alert">
        无法加载工作区：{{ workspace.error.message }}
      </p>
      <p v-else-if="workspace.notice?.kind === 'created'" class="banner" role="status">
        已创建新的工作区，预置了 development、staging、production 三个环境。
      </p>
      <p v-else-if="workspace.notice?.kind === 'migrated'" class="banner" role="status">
        工作区已从版本 {{ workspace.notice.from }} 升级，原文件已备份到数据目录的 backups 文件夹。
      </p>
      <RouterView />
    </main>
  </div>
</template>

<style scoped>
.shell {
  display: flex;
  height: 100%;
}

.content {
  flex: 1;
  min-width: 0;
  overflow: auto;
  padding: var(--space-5);
}

.banner {
  margin: 0 0 var(--space-4);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-control);
  background: var(--state-running-bg);
  color: var(--state-running);
}

.banner--failure {
  background: var(--state-failure-bg);
  color: var(--state-failure);
}
</style>

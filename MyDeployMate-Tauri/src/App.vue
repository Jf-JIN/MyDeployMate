<script setup lang="ts">
import { onBeforeUnmount, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import AppRail from './components/AppRail.vue'
import DirtyGuardDialog from './components/DirtyGuardDialog.vue'
import CloseRunDialog from './components/run/CloseRunDialog.vue'
import UiNotice from './components/ui/UiNotice.vue'
import UiToasts from './components/ui/UiToasts.vue'
import { openLayerCount } from './composables/focus'
import { useDraftStore } from './stores/drafts'
import { useLocaleStore } from './stores/locale'
import { useRunStore } from './stores/run'
import { useSessionStore } from './stores/session'
import { useThemeStore } from './stores/theme'
import { useWorkspaceStore } from './stores/workspace'

const workspace = useWorkspaceStore()
const session = useSessionStore()
const drafts = useDraftStore()
const runs = useRunStore()
useThemeStore()
useLocaleStore()
const route = useRoute()
const router = useRouter()

onMounted(async () => {
  // IXD-02: after a reload the active run's entry point comes back.
  void runs.restore()
  await workspace.load()
  await router.isReady()
  // IXD-02: start in the last project and pipeline; a deleted one falls back to the list.
  const last = workspace.projects.find((p) => p.id === session.lastProjectId)
  if (route.name === 'projects' && last) {
    await router.replace({ name: 'project', params: { projectId: last.id } })
  }
})

// IXD-03: Ctrl+S saves the current editor, except while a dialog owns the keyboard.
function onKeydown(event: KeyboardEvent) {
  if (event.ctrlKey && !event.altKey && event.key.toLowerCase() === 's') {
    event.preventDefault()
    if (openLayerCount() === 0) void drafts.saveCurrent()
  }
}
onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <div class="shell">
    <AppRail />
    <main class="main">
      <div v-if="workspace.error || workspace.notice" class="banners">
        <UiNotice v-if="workspace.error" tone="failure">
          无法加载工作区：{{ workspace.error.message }}
        </UiNotice>
        <UiNotice
          v-else-if="workspace.notice?.kind === 'created'"
          dismissible
          @dismiss="workspace.notice = null"
        >
          已创建新的工作区，预置了 development、staging、production 三个环境。
        </UiNotice>
        <UiNotice
          v-else-if="workspace.notice?.kind === 'migrated'"
          dismissible
          @dismiss="workspace.notice = null"
        >
          工作区已从版本 {{ workspace.notice.from }} 升级，原文件已备份到数据目录的 backups 文件夹。
        </UiNotice>
      </div>
      <RouterView />
    </main>
    <DirtyGuardDialog />
    <CloseRunDialog />
    <UiToasts />
  </div>
</template>

<style scoped>
.shell {
  display: flex;
  height: 100%;
}

.main {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--surface-panel);
}

.banners {
  flex: none;
  padding: var(--space-3) var(--space-5) 0;
}
</style>

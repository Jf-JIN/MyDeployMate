<script setup lang="ts">
import { onBeforeUnmount, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import AppRail from './components/AppRail.vue'
import DirtyGuardDialog from './components/DirtyGuardDialog.vue'
import CloseRunDialog from './components/run/CloseRunDialog.vue'
import UiNotice from './components/ui/UiNotice.vue'
import UiToasts from './components/ui/UiToasts.vue'
import WelcomeDialog from './components/WelcomeDialog.vue'
import { openLayerCount } from './composables/focus'
import { useT } from './i18n'
import { messageText } from './i18n/message'
import { useDraftStore } from './stores/drafts'
import { useLocaleStore } from './stores/locale'
import { useRunStore } from './stores/run'
import { useSessionStore } from './stores/session'
import { useThemeStore } from './stores/theme'
import { useWelcomeStore } from './stores/welcome'
import { useWorkspaceStore } from './stores/workspace'

const t = useT()

const workspace = useWorkspaceStore()
const session = useSessionStore()
const drafts = useDraftStore()
const runs = useRunStore()
useThemeStore()
useLocaleStore()
const welcome = useWelcomeStore()
const route = useRoute()
const router = useRouter()

onMounted(async () => {
  // IXD-02: after a reload the active run's entry point comes back.
  const restored = runs.restore()
  await workspace.load()
  await router.isReady()
  // IXD-02: start in the last project and pipeline; a deleted one falls back to the list.
  const last = workspace.projects.find((p) => p.id === session.lastProjectId)
  if (route.name === 'projects' && last) {
    await router.replace({ name: 'project', params: { projectId: last.id } })
  }
  // IXD-40: the welcome wizard on first start, unless a run is going on.
  await restored
  welcome.showOnStartup(!!runs.active)
})

// IXD-03: Ctrl+S saves the current editor, except while a dialog owns the keyboard.
function onKeydown(event: KeyboardEvent) {
  if (event.ctrlKey && !event.altKey && event.key.toLowerCase() === 's') {
    event.preventDefault()
    if (openLayerCount() === 0) void drafts.saveCurrent()
  }
}
// IXD-02 (v0.6): the first Tab stop skips the rail and the project tree.
function skipToContent() {
  const target =
    document.querySelector<HTMLElement>('.section-page') ?? document.querySelector('main')
  if (!target) return
  target.tabIndex = -1
  target.focus()
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <div class="shell">
    <a class="skip-link" href="#" @click.prevent="skipToContent">{{ t('app.skipToContent') }}</a>
    <AppRail />
    <main class="main">
      <div v-if="workspace.error || workspace.notice" class="banners">
        <UiNotice v-if="workspace.error" tone="failure">
          {{ t('app.workspace.loadFailed', { error: messageText(workspace.error.message) }) }}
        </UiNotice>
        <UiNotice
          v-else-if="workspace.notice?.kind === 'created'"
          dismissible
          @dismiss="workspace.notice = null"
        >
          {{ t('app.workspace.created') }}
        </UiNotice>
        <UiNotice
          v-else-if="workspace.notice?.kind === 'migrated'"
          dismissible
          @dismiss="workspace.notice = null"
        >
          {{ t('app.workspace.migrated', { from: workspace.notice.from }) }}
        </UiNotice>
      </div>
      <RouterView />
    </main>
    <DirtyGuardDialog />
    <CloseRunDialog />
    <WelcomeDialog />
    <UiToasts />
  </div>
</template>

<style scoped>
/* Off screen until it has keyboard focus. */
.skip-link {
  position: absolute;
  top: var(--space-2);
  left: var(--space-2);
  z-index: 200;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-control);
  background: var(--surface-panel);
  color: var(--action-primary);
  transform: translateY(-200%);
}

.skip-link:focus {
  transform: none;
}

.main :deep(.section-page:focus),
.main:focus {
  outline: none;
}

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

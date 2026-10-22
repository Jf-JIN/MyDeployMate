<script setup lang="ts">
import { Eye, Play, Save, Shield } from '@lucide/vue'
import { computed, ref } from 'vue'

import ActiveRunChip from '@/components/ActiveRunChip.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiTag from '@/components/ui/UiTag.vue'
import type { Pipeline, Project } from '@/ipc'
import { useDraftStore } from '@/stores/drafts'
import { useRunStore } from '@/stores/run'
import { useSessionStore } from '@/stores/session'
import { useWorkspaceStore } from '@/stores/workspace'

// UI-12 / IXD-01: the top bar always shows project, pipeline and environment, plus Save,
// Preview and Run. Switching environment goes through the leave guard (IXD-03).
const props = defineProps<{ project: Project; pipeline?: Pipeline | null }>()

const workspace = useWorkspaceStore()
const session = useSessionStore()
const drafts = useDraftStore()
const run = useRunStore()

const environmentId = computed(
  () => session.environment[props.project.id] ?? props.project.defaultEnvironmentId,
)
const environment = computed(() => workspace.environments.find((e) => e.id === environmentId.value))
const environmentOptions = computed(() =>
  workspace.environments.map((e) => ({ value: e.id, label: e.name })),
)

// Re-render the select when a guarded switch is cancelled so it shows the kept value.
const selectKey = ref(0)
const selectedEnvironment = computed({
  get: () => environmentId.value,
  set: async (next: string) => {
    if (next === environmentId.value) return
    if (await drafts.confirmLeave()) session.selectEnvironment(props.project.id, next)
    else selectKey.value++
  },
})

const saveReason = computed(() => (drafts.anyDirty ? null : '没有未保存的更改'))
const runReason = computed(() => {
  if (run.active) return '已有运行进行中'
  return '执行引擎尚未接入（阶段 4）'
})
</script>

<template>
  <header class="context-bar">
    <div class="context-title">
      <span v-if="pipeline" class="context-project" :title="project.name">{{ project.name }}</span>
      <h1 class="context-name" :title="pipeline?.name ?? project.name">
        {{ pipeline?.name ?? project.name }}
      </h1>
    </div>
    <UiTag v-if="drafts.anyDirty" tone="warning">未保存</UiTag>
    <span class="separator" aria-hidden="true" />
    <label class="context-env">
      <span class="context-env-label">环境</span>
      <UiSelect
        :key="selectKey"
        v-model="selectedEnvironment"
        class="context-env-select"
        :options="environmentOptions"
        compact
      />
    </label>
    <UiTag v-if="environment?.protected" tone="warning" :icon="Shield">保护环境</UiTag>
    <div class="spacer" />
    <ActiveRunChip />
    <UiButton
      :icon="Save"
      :busy="drafts.saving"
      :disabled-reason="saveReason"
      @click="drafts.saveCurrent()"
    >
      保存
    </UiButton>
    <template v-if="pipeline">
      <UiButton :icon="Eye" disabled-reason="发布预览将在阶段 3.7 接入">预览</UiButton>
      <UiButton variant="primary" :icon="Play" :disabled-reason="runReason">执行</UiButton>
    </template>
  </header>
</template>

<style scoped>
.context-bar {
  display: flex;
  flex: none;
  align-items: center;
  gap: var(--space-3);
  height: var(--context-bar-height);
  padding: 0 var(--space-5);
  border-bottom: 1px solid var(--border-subtle);
  background: var(--surface-panel);
}

.context-title {
  display: flex;
  flex-direction: column;
  min-width: 0;
  max-width: 360px;
}

.context-project {
  overflow: hidden;
  color: var(--text-secondary);
  font: var(--font-caption);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.context-name {
  overflow: hidden;
  font: var(--font-title);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.context-project + .context-name {
  font-size: 18px;
  line-height: 24px;
}

.separator {
  width: 1px;
  height: 24px;
  background: var(--border-subtle);
}

.context-env {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.context-env-label {
  color: var(--text-secondary);
  font: var(--font-caption);
}

/* Environment names are never cut so short that production and staging look alike (UI-12). */
.context-env-select {
  width: 168px;
}

.spacer {
  flex: 1;
}
</style>

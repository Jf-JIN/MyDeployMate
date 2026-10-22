<script setup lang="ts">
import { computed, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import ProjectContextBar from '@/components/ProjectContextBar.vue'
import { useSessionStore } from '@/stores/session'
import { useWorkspaceStore } from '@/stores/workspace'

// A project's pages: context bar, second-level tabs (IXD-02) and the routed tab.
const route = useRoute()
const router = useRouter()
const workspace = useWorkspaceStore()
const session = useSessionStore()

const project = computed(() =>
  workspace.projects.find((p) => p.id === (route.params.projectId as string)),
)
const pipeline = computed(() => {
  const id = route.params.pipelineId as string | undefined
  return id ? (project.value?.pipelines.find((p) => p.id === id) ?? null) : null
})

const pipelineTabTarget = computed(() => {
  const p = project.value
  if (!p) return null
  const remembered = session.lastPipeline[p.id]
  const target = p.pipelines.find((x) => x.id === remembered) ?? p.pipelines[0]
  return target
    ? { name: 'pipeline', params: { projectId: p.id, pipelineId: target.id } }
    : { name: 'project-pipelines', params: { projectId: p.id } }
})

const tabs = computed(() => {
  const p = project.value
  if (!p || !pipelineTabTarget.value) return []
  const environmentId = session.environment[p.id] ?? p.defaultEnvironmentId
  const targets = p.targets.find((t) => t.environmentId === environmentId)?.serverIds.length ?? 0
  return [
    { key: 'pipeline', label: '执行流程', count: p.pipelines.length, to: pipelineTabTarget.value },
    { key: 'targets', label: '目标服务器', count: targets, to: { name: 'project-targets' } },
    {
      key: 'variables',
      label: '变量与环境',
      count: p.variables.length,
      to: { name: 'project-variables' },
    },
    { key: 'settings', label: '项目设置', count: null, to: { name: 'project-settings' } },
  ]
})

const activeTab = computed(() => {
  const name = String(route.name)
  if (name === 'pipeline' || name === 'project-pipelines') return 'pipeline'
  return name.replace('project-', '')
})

// IXD-02: a deleted project or pipeline sends the user back instead of into something else.
watch(
  [() => workspace.workspace, project, pipeline, () => route.name],
  () => {
    if (!workspace.workspace) return
    if (!project.value) {
      void router.replace({ name: 'projects' })
    } else if (route.name === 'pipeline' && !pipeline.value) {
      void router.replace({ name: 'project', params: { projectId: project.value.id } })
    } else if (route.name === 'project' && pipelineTabTarget.value) {
      void router.replace(pipelineTabTarget.value)
    } else if (pipeline.value) {
      session.openPipeline(project.value.id, pipeline.value.id)
    }
  },
  { immediate: true },
)
</script>

<template>
  <template v-if="project">
    <ProjectContextBar :project="project" :pipeline="pipeline" />
    <nav class="tabs" aria-label="项目视图">
      <RouterLink
        v-for="tab in tabs"
        :key="tab.key"
        :to="tab.to"
        class="tab"
        :class="{ 'is-active': activeTab === tab.key }"
        :aria-current="activeTab === tab.key ? 'page' : undefined"
      >
        {{ tab.label }}
        <span v-if="tab.count !== null" class="tab-count num">{{ tab.count }}</span>
      </RouterLink>
    </nav>
    <div class="project-page">
      <RouterView :project="project" :pipeline="pipeline" />
    </div>
  </template>
</template>

<style scoped>
.tabs {
  display: flex;
  flex: none;
  gap: var(--space-1);
  height: var(--project-tabs-height);
  padding: 0 var(--space-5);
  border-bottom: 1px solid var(--border-subtle);
  background: var(--surface-panel);
}

/* UI-18: selected tab uses weight and an underline, not colour alone. */
.tab {
  display: inline-flex;
  align-items: center;
  padding: 0 var(--space-3);
  color: var(--text-secondary);
  font: var(--font-label);
  text-decoration: none;
  box-shadow: inset 0 -2px 0 transparent;
}

.tab:hover {
  color: var(--text-primary);
}

.tab.is-active {
  color: var(--text-primary);
  font-weight: 600;
  box-shadow: inset 0 -2px 0 var(--action-primary);
}

.tab-count {
  margin-left: 6px;
  padding: 0 6px;
  border-radius: 9px;
  background: var(--surface-hover);
  color: var(--text-secondary);
  font: var(--font-caption);
}

.project-page {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  overflow: auto;
  padding: var(--space-4) var(--space-5);
}
</style>

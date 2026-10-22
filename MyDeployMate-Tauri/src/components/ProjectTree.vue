<script setup lang="ts">
import { ChevronDown, ChevronRight, PanelLeftClose, Plus, Search } from '@lucide/vue'
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import PipelineDialog from '@/components/pipeline/PipelineDialog.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import type { Project } from '@/ipc'
import { useDraftStore } from '@/stores/drafts'
import { useSessionStore } from '@/stores/session'
import { useWorkspaceStore } from '@/stores/workspace'

// IXD-02 (v0.3): project → pipeline tree beside the rail; a pipeline opens P02 in one click.
const workspace = useWorkspaceStore()
const session = useSessionStore()
const route = useRoute()
const router = useRouter()
const drafts = useDraftStore()

const query = ref('')
const expanded = ref(new Set<string>())

const currentProjectId = computed(() => route.params.projectId as string | undefined)
const currentPipelineId = computed(() => route.params.pipelineId as string | undefined)

watch(
  currentProjectId,
  (id) => {
    if (id && !expanded.value.has(id)) expanded.value = new Set([...expanded.value, id])
  },
  { immediate: true },
)

function matches(text: string) {
  return text.toLowerCase().includes(query.value.trim().toLowerCase())
}

const visible = computed(() => {
  const q = query.value.trim()
  return workspace.projects
    .map((project) => {
      if (!q || matches(project.name) || matches(project.description)) {
        return { project, pipelines: project.pipelines }
      }
      return { project, pipelines: project.pipelines.filter((p) => matches(p.name)) }
    })
    .filter((entry) => !q || entry.pipelines.length > 0 || matches(entry.project.name))
})

function isOpen(project: Project) {
  return query.value.trim() !== '' || expanded.value.has(project.id)
}

function toggle(project: Project) {
  const next = new Set(expanded.value)
  if (next.has(project.id)) next.delete(project.id)
  else next.add(project.id)
  expanded.value = next
}

// IXD-02: "新建流程" under an expanded project. It saves at once, so drafts are settled first.
const creatingFor = ref<Project | null>(null)
async function createPipeline(project: Project) {
  if (await drafts.confirmLeave()) creatingFor.value = project
}
function onCreated(pipelineId: string) {
  const projectId = creatingFor.value?.id
  creatingFor.value = null
  if (projectId) void router.push({ name: 'pipeline', params: { projectId, pipelineId } })
}

function environmentLabel(project: Project) {
  const id = session.environment[project.id] ?? project.defaultEnvironmentId
  return workspace.environmentName(id)
}
</script>

<template>
  <aside class="tree" aria-label="项目与流程">
    <header class="tree-header">
      <h2 class="tree-title">项目与流程</h2>
      <slot name="actions" />
      <UiIconButton
        :icon="PanelLeftClose"
        label="收起项目树"
        @click="session.treeCollapsed = true"
      />
    </header>
    <div class="tree-search">
      <Search class="tree-search-icon" :size="16" :stroke-width="1.75" aria-hidden="true" />
      <input
        v-model="query"
        class="control tree-search-input"
        type="search"
        placeholder="搜索项目或流程"
        aria-label="搜索项目或流程"
      />
    </div>
    <ul v-if="visible.length" class="tree-list" role="tree" aria-label="项目">
      <li
        v-for="{ project, pipelines } in visible"
        :key="project.id"
        role="treeitem"
        :aria-expanded="isOpen(project)"
        :aria-selected="project.id === currentProjectId && !currentPipelineId"
      >
        <div
          class="tree-row tree-project"
          :class="{ 'is-current': project.id === currentProjectId }"
        >
          <button
            type="button"
            class="tree-toggle"
            :aria-label="`${isOpen(project) ? '收起' : '展开'} ${project.name}`"
            @click="toggle(project)"
          >
            <component
              :is="isOpen(project) ? ChevronDown : ChevronRight"
              :size="16"
              :stroke-width="1.75"
              aria-hidden="true"
            />
          </button>
          <RouterLink
            class="tree-link"
            :to="{ name: 'project', params: { projectId: project.id } }"
            :title="project.name"
          >
            <span class="tree-name">{{ project.name }}</span>
            <span class="tree-env">{{ environmentLabel(project) }}</span>
          </RouterLink>
        </div>
        <ul v-if="isOpen(project)" role="group">
          <li
            v-for="pipeline in pipelines"
            :key="pipeline.id"
            role="treeitem"
            :aria-selected="pipeline.id === currentPipelineId"
          >
            <RouterLink
              class="tree-row tree-pipeline"
              :class="{ 'is-selected': pipeline.id === currentPipelineId }"
              :to="{ name: 'pipeline', params: { projectId: project.id, pipelineId: pipeline.id } }"
              :title="pipeline.name"
            >
              <span class="tree-name">{{ pipeline.name }}</span>
            </RouterLink>
          </li>
          <li v-if="pipelines.length === 0" class="tree-empty">尚无流程</li>
          <li v-if="!query.trim()" role="none">
            <button
              type="button"
              class="tree-row tree-pipeline tree-new"
              @click="createPipeline(project)"
            >
              <Plus :size="14" :stroke-width="1.75" aria-hidden="true" />
              <span class="tree-name">新建流程</span>
            </button>
          </li>
        </ul>
      </li>
    </ul>
    <p v-else-if="workspace.projects.length" class="tree-empty tree-empty--block">
      没有匹配“{{ query }}”的项目或流程
      <button type="button" class="tree-clear" @click="query = ''">清除搜索</button>
    </p>
    <p v-else class="tree-empty tree-empty--block">尚无项目</p>
    <div v-if="$slots.footer" class="tree-footer"><slot name="footer" /></div>
    <PipelineDialog
      v-if="creatingFor"
      mode="create"
      :project="creatingFor"
      @cancel="creatingFor = null"
      @saved="onCreated"
    />
  </aside>
</template>

<style scoped>
.tree {
  display: flex;
  flex: none;
  flex-direction: column;
  width: var(--tree-width);
  min-height: 0;
  border-right: 1px solid var(--border-subtle);
  background: var(--surface-panel);
}

.tree-header {
  display: flex;
  flex: none;
  align-items: center;
  gap: var(--space-1);
  height: var(--context-bar-height);
  padding: 0 var(--space-2) 0 var(--space-4);
  border-bottom: 1px solid var(--border-subtle);
}

.tree-title {
  flex: 1;
  font: var(--font-section);
}

.tree-search {
  position: relative;
  padding: var(--space-3);
}

.tree-search-icon {
  position: absolute;
  top: 50%;
  left: 24px;
  color: var(--text-secondary);
  transform: translateY(-50%);
  pointer-events: none;
}

.tree-search-input {
  height: var(--button-height-compact);
  padding-left: 34px;
}

.tree-list {
  flex: 1;
  min-height: 0;
  margin: 0;
  padding: 0 var(--space-2) var(--space-3);
  overflow: auto;
  list-style: none;
}

.tree-list ul {
  margin: 0;
  padding: 0;
  list-style: none;
}

.tree-row {
  display: flex;
  align-items: center;
  min-height: var(--menu-item-height);
  border-radius: var(--radius-control);
  color: var(--text-primary);
  text-decoration: none;
}

.tree-row:hover {
  background: var(--surface-hover);
}

.tree-project.is-current .tree-name {
  font-weight: 600;
}

.tree-toggle {
  display: inline-flex;
  flex: none;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: var(--menu-item-height);
  padding: 0;
  border: 0;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}

.tree-link {
  display: flex;
  flex: 1;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
  height: var(--menu-item-height);
  padding-right: var(--space-2);
  color: inherit;
  text-decoration: none;
}

.tree-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tree-env {
  flex: none;
  max-width: 88px;
  overflow: hidden;
  color: var(--text-secondary);
  font: var(--font-caption);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tree-pipeline {
  padding: 0 var(--space-2) 0 36px;
}

.tree-pipeline.is-selected {
  background: var(--surface-selected);
  box-shadow: inset 3px 0 0 var(--action-primary);
  font-weight: 600;
}

.tree-empty {
  padding: var(--space-1) var(--space-2) var(--space-1) 36px;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.tree-new {
  gap: var(--space-1);
  width: 100%;
  border: 0;
  background: transparent;
  color: var(--text-secondary);
  font: var(--font-body);
  text-align: left;
  cursor: pointer;
}

.tree-empty--block {
  margin: 0;
  padding: var(--space-2) var(--space-4);
}

.tree-clear {
  display: block;
  margin-top: var(--space-1);
  padding: 0;
  border: 0;
  background: none;
  color: var(--action-primary);
  font: inherit;
  cursor: pointer;
}

.tree-footer {
  flex: none;
  padding: var(--space-3);
  border-top: 1px solid var(--border-subtle);
}
</style>

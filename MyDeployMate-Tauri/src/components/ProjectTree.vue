<script setup lang="ts">
import { ChevronDown, ChevronRight, PanelLeftClose, Plus, Search } from '@lucide/vue'
import { computed, nextTick, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import PipelineDialog from '@/components/pipeline/PipelineDialog.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import { useT } from '@/i18n'
import type { Project } from '@/ipc'
import { useDraftStore } from '@/stores/drafts'
import { useSessionStore } from '@/stores/session'
import { useWorkspaceStore } from '@/stores/workspace'

const t = useT()

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

// ---- Keyboard (IXD-02 v0.6) ------------------------------------------------------------------
// The tree is one Tab stop: the active item has tabindex 0, the rest -1, and the arrow keys move
// between the visible projects, pipelines and "新建流程" rows.

const list = ref<HTMLElement | null>(null)
const activeKey = ref<string | null>(null)

const keys = computed(() => {
  const out: string[] = []
  for (const { project, pipelines } of visible.value) {
    out.push(`p:${project.id}`)
    if (!isOpen(project)) continue
    for (const pipeline of pipelines) out.push(`l:${pipeline.id}`)
    if (!query.value.trim()) out.push(`n:${project.id}`)
  }
  return out
})

/** The item that takes Tab: the last focused one if still shown, else the current route's. */
const tabKey = computed(() => {
  const shown = keys.value
  const candidates = [
    activeKey.value,
    currentPipelineId.value ? `l:${currentPipelineId.value}` : null,
    currentProjectId.value ? `p:${currentProjectId.value}` : null,
  ]
  return candidates.find((k) => k && shown.includes(k)) ?? shown[0] ?? null
})

function tabindex(key: string) {
  return key === tabKey.value ? 0 : -1
}

async function focusKey(key: string | undefined) {
  if (!key) return
  activeKey.value = key
  await nextTick()
  list.value?.querySelector<HTMLElement>(`[data-key="${key}"]`)?.focus()
}

function onFocusin(event: FocusEvent) {
  const key = (event.target as HTMLElement).closest<HTMLElement>('[data-key]')?.dataset.key
  if (key) activeKey.value = key
}

function projectOf(key: string): Project | undefined {
  const [kind, id] = [key.slice(0, 1), key.slice(2)]
  if (kind === 'p' || kind === 'n') return workspace.projects.find((p) => p.id === id)
  return workspace.projects.find((p) => p.pipelines.some((l) => l.id === id))
}

function onKeydown(event: KeyboardEvent) {
  const key = (event.target as HTMLElement).closest<HTMLElement>('[data-key]')?.dataset.key
  if (!key || event.altKey || event.ctrlKey || event.metaKey) return
  const shown = keys.value
  const at = shown.indexOf(key)
  const project = projectOf(key)
  let next: string | undefined
  switch (event.key) {
    case 'ArrowDown':
      next = shown[at + 1]
      break
    case 'ArrowUp':
      next = shown[at - 1]
      break
    case 'Home':
      next = shown[0]
      break
    case 'End':
      next = shown[shown.length - 1]
      break
    case 'ArrowRight':
      if (!key.startsWith('p:') || !project) return
      if (!isOpen(project)) toggle(project)
      else next = shown[at + 1]
      break
    case 'ArrowLeft':
      if (!project) return
      if (key.startsWith('p:')) {
        if (isOpen(project) && !query.value.trim()) toggle(project)
      } else next = `p:${project.id}`
      break
    default:
      return
  }
  event.preventDefault()
  void focusKey(next)
}

function environmentLabel(project: Project) {
  const id = session.environment[project.id] ?? project.defaultEnvironmentId
  return workspace.environmentName(id)
}
</script>

<template>
  <aside class="tree" :aria-label="t('tree.title')">
    <header class="tree-header">
      <h2 class="tree-title">{{ t('tree.title') }}</h2>
      <slot name="actions" />
      <UiIconButton
        :icon="PanelLeftClose"
        :label="t('tree.collapse')"
        @click="session.treeCollapsed = true"
      />
    </header>
    <div class="tree-search">
      <Search class="tree-search-icon" :size="16" :stroke-width="1.75" aria-hidden="true" />
      <input
        v-model="query"
        class="control tree-search-input"
        type="search"
        :placeholder="t('tree.search')"
        :aria-label="t('tree.search')"
      />
    </div>
    <ul
      v-if="visible.length"
      ref="list"
      class="tree-list"
      role="tree"
      :aria-label="t('tree.projects')"
      @keydown="onKeydown"
      @focusin="onFocusin"
    >
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
            tabindex="-1"
            :aria-label="
              t(isOpen(project) ? 'tree.collapseItem' : 'tree.expandItem', { name: project.name })
            "
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
            :data-key="`p:${project.id}`"
            :tabindex="tabindex(`p:${project.id}`)"
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
              :data-key="`l:${pipeline.id}`"
              :tabindex="tabindex(`l:${pipeline.id}`)"
            >
              <span class="tree-name">{{ pipeline.name }}</span>
            </RouterLink>
          </li>
          <li v-if="pipelines.length === 0" class="tree-empty">{{ t('tree.noPipelines') }}</li>
          <li v-if="!query.trim()" role="none">
            <button
              type="button"
              class="tree-row tree-pipeline tree-new"
              :data-key="`n:${project.id}`"
              :tabindex="tabindex(`n:${project.id}`)"
              @click="createPipeline(project)"
            >
              <Plus :size="14" :stroke-width="1.75" aria-hidden="true" />
              <span class="tree-name">{{ t('tree.newPipeline') }}</span>
            </button>
          </li>
        </ul>
      </li>
    </ul>
    <p v-else-if="workspace.projects.length" class="tree-empty tree-empty--block">
      {{ t('tree.noMatch', { query }) }}
      <button type="button" class="tree-clear" @click="query = ''">
        {{ t('tree.clearSearch') }}
      </button>
    </p>
    <p v-else class="tree-empty tree-empty--block">{{ t('tree.noProjects') }}</p>
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
  background: var(--surface-sidebar);
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
  background: var(--surface-sidebar-hover);
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

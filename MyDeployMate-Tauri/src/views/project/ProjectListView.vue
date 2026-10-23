<script setup lang="ts">
import { Check, Copy, FolderKanban, Plus, Search, SearchX, Trash2 } from '@lucide/vue'
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'

import EmptyState from '@/components/EmptyState.vue'
import PageLayout from '@/components/PageLayout.vue'
import ProjectDialog from '@/components/projects/ProjectDialog.vue'
import { boundTargetCount } from '@/components/projects/projectStats'
import UiButton from '@/components/ui/UiButton.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import { cloneWorkspace } from '@/composables/workspaceDraft'
import { useT } from '@/i18n'
import { toAppError, type Project } from '@/ipc'
import { useRunStore } from '@/stores/run'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'
import { useWorkspaceStore } from '@/stores/workspace'

const t = useT()

// P01 project list (IXD-06, IXD-07). Run results join with run history in phase 4.
const store = useWorkspaceStore()
const session = useSessionStore()
const run = useRunStore()
const toast = useToastStore()
const router = useRouter()

// IXD-06: a click anywhere on a row opens the project, except on its own controls.
function openFromRow(event: MouseEvent, project: Project) {
  if ((event.target as Element).closest('a, button')) return
  void router.push({ name: 'project', params: { projectId: project.id } })
}

const query = ref('')
const visible = computed(() => {
  const q = query.value.trim().toLowerCase()
  const sorted = [...store.projects].sort((a, b) =>
    a.name.localeCompare(b.name, 'zh-CN', { numeric: true }),
  )
  return q ? sorted.filter((p) => `${p.name}\n${p.description}`.toLowerCase().includes(q)) : sorted
})

// First-run guide on the empty page: servers can be added before or after the project.
const guide = computed(() => [
  {
    title: t('projects.guide.servers.title'),
    detail: t('projects.guide.servers.detail'),
    done: store.servers.length > 0,
    to: { name: 'servers' },
    link: t('projects.guide.servers.link'),
  },
  {
    title: t('projects.guide.project.title'),
    detail: t('projects.guide.project.detail'),
    done: false,
  },
  {
    title: t('projects.guide.pipeline.title'),
    detail: t('projects.guide.pipeline.detail'),
    done: false,
  },
])

function currentEnvironment(project: Project) {
  return session.environment[project.id] ?? project.defaultEnvironmentId
}

function currentTargets(project: Project) {
  const env = currentEnvironment(project)
  return project.targets.find((b) => b.environmentId === env)?.serverIds.length ?? 0
}

// ---- Create and copy ----------------------------------------------------------------------

const dialog = ref<'create' | 'copy' | null>(null)
const source = ref<Project | null>(null)

function create() {
  source.value = null
  dialog.value = 'create'
}

function copy(project: Project) {
  source.value = project
  dialog.value = 'copy'
}

async function onSaved(project: Project) {
  const copied = dialog.value === 'copy'
  dialog.value = null
  toast.show(t(copied ? 'projects.copied' : 'projects.created', { name: project.name }))
  const first = project.pipelines[0]
  await router.push(
    first
      ? { name: 'pipeline', params: { projectId: project.id, pipelineId: first.id } }
      : { name: 'project', params: { projectId: project.id } },
  )
}

// ---- Delete --------------------------------------------------------------------------------

const deleting = ref<Project | null>(null)
const deleteBusy = ref(false)
const listError = ref<string | null>(null)
const deleteReason = computed(() => (run.active ? t('projects.deleteBlocked') : null))

async function applyDelete() {
  const project = deleting.value
  if (!project || !store.workspace) return
  deleteBusy.value = true
  try {
    const draft = cloneWorkspace(store.workspace)
    draft.projects = draft.projects.filter((p) => p.id !== project.id)
    await store.save(draft)
    toast.show(t('projects.deleted', { name: project.name }))
  } catch (e) {
    listError.value = toAppError(e).message
  } finally {
    deleteBusy.value = false
    deleting.value = null
  }
}
</script>

<template>
  <PageLayout :title="t('projects.title')">
    <template #actions>
      <UiButton variant="primary" :icon="Plus" @click="create">
        {{ t('projects.create') }}
      </UiButton>
    </template>

    <div class="projects">
      <UiNotice v-if="listError" tone="failure" dismissible @dismiss="listError = null">
        {{ listError }}
      </UiNotice>

      <template v-if="store.projects.length">
        <div class="filters" role="search">
          <div class="search">
            <Search class="search-icon" :size="16" :stroke-width="1.75" aria-hidden="true" />
            <UiTextInput
              v-model="query"
              :placeholder="t('projects.list.search')"
              :aria-label="t('projects.list.searchLabel')"
            />
          </div>
          <span class="count num">{{
            t('projects.list.count', { shown: visible.length, total: store.projects.length })
          }}</span>
        </div>

        <div class="table-wrap">
          <table v-if="visible.length" class="table">
            <thead>
              <tr>
                <th scope="col" class="col-name">{{ t('common.name') }}</th>
                <th scope="col">{{ t('common.description') }}</th>
                <th scope="col">{{ t('projects.dialog.defaultEnvironment') }}</th>
                <th scope="col" class="num">{{ t('projects.list.pipelines') }}</th>
                <th scope="col">{{ t('projects.list.currentTargets') }}</th>
                <th scope="col">{{ t('projects.list.lastRun') }}</th>
                <th scope="col" class="col-actions">
                  <span class="sr-only">{{ t('common.actions') }}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="project in visible"
                :key="project.id"
                class="row"
                @click="openFromRow($event, project)"
              >
                <td class="col-name">
                  <RouterLink
                    class="name"
                    :to="{ name: 'project', params: { projectId: project.id } }"
                  >
                    {{ project.name }}
                  </RouterLink>
                </td>
                <td class="col-desc" :title="project.description">{{ project.description }}</td>
                <td>{{ store.environmentName(project.defaultEnvironmentId) }}</td>
                <td class="num">{{ project.pipelines.length }}</td>
                <i18n-t keypath="projects.list.targetsCell" tag="td">
                  <template #environment>
                    {{ store.environmentName(currentEnvironment(project)) }}
                  </template>
                  <template #count>
                    <span class="num">{{ currentTargets(project) }}</span>
                  </template>
                </i18n-t>
                <td class="muted">{{ t('projects.list.noRuns') }}</td>
                <td class="col-actions">
                  <span class="actions">
                    <UiIconButton
                      :icon="Copy"
                      :label="t('projects.list.copyItem', { name: project.name })"
                      @click="copy(project)"
                    />
                    <UiIconButton
                      :icon="Trash2"
                      :label="t('projects.list.deleteItem', { name: project.name })"
                      :disabled-reason="deleteReason"
                      @click="deleting = project"
                    />
                  </span>
                </td>
              </tr>
            </tbody>
          </table>
          <EmptyState v-else :title="t('projects.list.noMatch')" :icon="SearchX" compact>
            <UiButton compact @click="query = ''">{{ t('tree.clearSearch') }}</UiButton>
          </EmptyState>
        </div>
      </template>

      <EmptyState
        v-else
        :title="t('projects.list.emptyTitle')"
        :detail="t('projects.list.emptyDetail')"
        :icon="FolderKanban"
      >
        <UiButton variant="primary" :icon="Plus" @click="create">
          {{ t('projects.create') }}
        </UiButton>
        <template #after>
          <ol class="guide" :aria-label="t('projects.guide.label')">
            <li v-for="(step, i) in guide" :key="step.title" :class="{ done: step.done }">
              <span class="guide-mark" aria-hidden="true">
                <Check v-if="step.done" :size="14" :stroke-width="2.5" />
                <template v-else>{{ i + 1 }}</template>
              </span>
              <span class="guide-text">
                <span class="guide-title">
                  {{ step.title
                  }}<span v-if="step.done" class="sr-only">{{ t('projects.guide.done') }}</span>
                </span>
                <span class="guide-detail">{{ step.detail }}</span>
              </span>
              <RouterLink v-if="step.to && !step.done" :to="step.to" class="guide-link">
                {{ step.link }}
              </RouterLink>
            </li>
          </ol>
        </template>
      </EmptyState>
    </div>

    <ProjectDialog :mode="dialog" :source="source" @cancel="dialog = null" @saved="onSaved" />

    <UiConfirm
      :open="!!deleting"
      :title="t('projects.delete.title', { name: deleting?.name ?? '' })"
      :confirm-label="t('projects.delete.confirm')"
      danger
      :busy="deleteBusy"
      @cancel="deleting = null"
      @confirm="applyDelete"
    >
      <p v-if="deleting">
        {{
          t('projects.delete.counts', {
            pipelines: deleting.pipelines.length,
            targets: boundTargetCount(deleting),
          })
        }}
      </p>
      <p class="muted">{{ t('projects.delete.localOnly') }}</p>
    </UiConfirm>
  </PageLayout>
</template>

<style scoped>
.projects {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.filters {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.search {
  position: relative;
  flex: 1;
  max-width: 360px;
}

.search :deep(.control) {
  padding-left: 34px;
}

.search-icon {
  position: absolute;
  top: 50%;
  left: 12px;
  z-index: 1;
  color: var(--text-secondary);
  transform: translateY(-50%);
  pointer-events: none;
}

.count {
  margin-left: auto;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.table-wrap {
  overflow-x: auto;
}

.table {
  min-width: 820px;
}

.col-name {
  min-width: 160px;
}

/* The whole row opens the project; it lights up on hover, and the name takes the accent. */
.row {
  cursor: pointer;
}

.row:hover > td {
  background: var(--surface-hover);
}

/* The name stays the keyboard way in, styled as plain text. */
.name {
  color: var(--text-primary);
  font-weight: 500;
  text-decoration: none;
}

.row:hover .name {
  color: var(--action-primary);
}

.col-desc {
  max-width: 280px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.col-actions {
  width: 88px;
}

.actions {
  display: inline-flex;
  gap: var(--space-1);
}

.muted {
  color: var(--text-secondary);
}

.guide {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  width: 100%;
  margin: var(--space-5) 0 0;
  padding: var(--space-4);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-surface);
  background: var(--surface-panel);
  list-style: none;
  text-align: left;
}

.guide li {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
}

.guide-mark {
  display: inline-flex;
  flex: none;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: 1px solid var(--border-control);
  border-radius: 50%;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.done .guide-mark {
  border-color: var(--state-success);
  background: var(--state-success-bg);
  color: var(--state-success);
}

.guide-text {
  display: flex;
  flex: 1;
  flex-direction: column;
}

.guide-title {
  color: var(--text-primary);
  font: var(--font-label);
}

.guide-detail {
  font: var(--font-caption);
}

.guide-link {
  flex: none;
  color: var(--action-primary);
  font: var(--font-label);
}
</style>

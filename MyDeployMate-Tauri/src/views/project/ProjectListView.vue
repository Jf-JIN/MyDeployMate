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
import { toAppError, type Project } from '@/ipc'
import { useRunStore } from '@/stores/run'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'
import { useWorkspaceStore } from '@/stores/workspace'

// P01 project list (IXD-06, IXD-07). Run results join with run history in phase 4.
const store = useWorkspaceStore()
const session = useSessionStore()
const run = useRunStore()
const toast = useToastStore()
const router = useRouter()

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
    title: '添加服务器',
    detail: '录入要部署到的 Linux 服务器和登录方式。',
    done: store.servers.length > 0,
    to: { name: 'servers' },
    link: '前往服务器',
  },
  {
    title: '新建项目',
    detail: '选择默认环境，再为各环境勾选目标服务器。',
    done: false,
  },
  {
    title: '编辑流程',
    detail: '添加本地命令、上传文件、远程命令和人工确认，先预览再执行。',
    done: false,
  },
])

function currentEnvironment(project: Project) {
  return session.environment[project.id] ?? project.defaultEnvironmentId
}

function currentTargets(project: Project) {
  const env = currentEnvironment(project)
  return project.targets.find((t) => t.environmentId === env)?.serverIds.length ?? 0
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
  toast.show(copied ? `已复制为 ${project.name}` : `已创建 ${project.name}`)
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
const deleteReason = computed(() => (run.active ? '有运行进行中，结束后才能删除项目' : null))

async function applyDelete() {
  const project = deleting.value
  if (!project || !store.workspace) return
  deleteBusy.value = true
  try {
    const draft = cloneWorkspace(store.workspace)
    draft.projects = draft.projects.filter((p) => p.id !== project.id)
    await store.save(draft)
    toast.show(`已删除 ${project.name}`)
  } catch (e) {
    listError.value = toAppError(e).message
  } finally {
    deleteBusy.value = false
    deleting.value = null
  }
}
</script>

<template>
  <PageLayout title="项目">
    <template #actions>
      <UiButton variant="primary" :icon="Plus" @click="create">新建项目</UiButton>
    </template>

    <div class="projects">
      <UiNotice v-if="listError" tone="failure" dismissible @dismiss="listError = null">
        {{ listError }}
      </UiNotice>

      <template v-if="store.projects.length">
        <div class="filters" role="search">
          <div class="search">
            <Search class="search-icon" :size="16" :stroke-width="1.75" aria-hidden="true" />
            <UiTextInput v-model="query" placeholder="搜索名称或说明" aria-label="搜索项目" />
          </div>
          <span class="count num">{{ visible.length }} / {{ store.projects.length }} 个</span>
        </div>

        <div class="table-wrap">
          <table v-if="visible.length" class="table">
            <thead>
              <tr>
                <th scope="col" class="col-name">名称</th>
                <th scope="col">说明</th>
                <th scope="col">默认环境</th>
                <th scope="col" class="num">流程</th>
                <th scope="col">当前环境目标</th>
                <th scope="col">最近运行</th>
                <th scope="col" class="col-actions"><span class="sr-only">操作</span></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="project in visible" :key="project.id">
                <td class="col-name">
                  <RouterLink :to="{ name: 'project', params: { projectId: project.id } }">
                    {{ project.name }}
                  </RouterLink>
                </td>
                <td class="col-desc" :title="project.description">{{ project.description }}</td>
                <td>{{ store.environmentName(project.defaultEnvironmentId) }}</td>
                <td class="num">{{ project.pipelines.length }}</td>
                <td>
                  {{ store.environmentName(currentEnvironment(project)) }} ·
                  <span class="num">{{ currentTargets(project) }}</span> 台
                </td>
                <td class="muted">尚无运行记录</td>
                <td class="col-actions">
                  <span class="actions">
                    <UiIconButton
                      :icon="Copy"
                      :label="`复制 ${project.name}`"
                      @click="copy(project)"
                    />
                    <UiIconButton
                      :icon="Trash2"
                      :label="`删除 ${project.name}`"
                      :disabled-reason="deleteReason"
                      @click="deleting = project"
                    />
                  </span>
                </td>
              </tr>
            </tbody>
          </table>
          <EmptyState v-else title="没有符合条件的项目" :icon="SearchX" compact>
            <UiButton compact @click="query = ''">清除搜索</UiButton>
          </EmptyState>
        </div>
      </template>

      <EmptyState
        v-else
        title="尚无项目"
        detail="项目把目标服务器、变量和发布流程组织在一起，按环境分别发布。"
        :icon="FolderKanban"
      >
        <UiButton variant="primary" :icon="Plus" @click="create">新建项目</UiButton>
        <template #after>
          <ol class="guide" aria-label="开始使用">
            <li v-for="(step, i) in guide" :key="step.title" :class="{ done: step.done }">
              <span class="guide-mark" aria-hidden="true">
                <Check v-if="step.done" :size="14" :stroke-width="2.5" />
                <template v-else>{{ i + 1 }}</template>
              </span>
              <span class="guide-text">
                <span class="guide-title">
                  {{ step.title }}<span v-if="step.done" class="sr-only">（已完成）</span>
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
      :title="`删除项目 ${deleting?.name ?? ''}`"
      confirm-label="删除项目"
      danger
      :busy="deleteBusy"
      @cancel="deleting = null"
      @confirm="applyDelete"
    >
      <p v-if="deleting">
        将删除 {{ deleting.pipelines.length }} 个流程和
        {{ boundTargetCount(deleting) }} 个目标绑定。
      </p>
      <p class="muted">只删除本地配置，不会删除服务器上的目录或服务；运行历史会保留。</p>
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
  font-weight: 600;
}

/* The name opens the project, but reads as a bold label rather than a link. */
.col-name a {
  color: var(--text-primary);
  text-decoration: none;
}

.col-name a:hover {
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

<script setup lang="ts">
import { CircleAlert, Copy, Shield } from '@lucide/vue'
import { computed, nextTick, ref, useId, watch } from 'vue'

import { hostPort } from '@/components/servers/serverForm'
import UiButton from '@/components/ui/UiButton.vue'
import UiCheckbox from '@/components/ui/UiCheckbox.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiField from '@/components/ui/UiField.vue'
import UiIconButton from '@/components/ui/UiIconButton.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import UiTag from '@/components/ui/UiTag.vue'
import UiTextInput from '@/components/ui/UiTextInput.vue'
import {
  call,
  toAppError,
  type Issue,
  type PlanRequest,
  type PlannedTask,
  type ResolvedPlan,
  type RunScope,
  type Workspace,
} from '@/ipc'
import { useShellPrograms } from '@/composables/shellPrograms'
import { useRunStore } from '@/stores/run'
import { useToastStore } from '@/stores/toast'

import { groupTasks, issueTarget, taskCommand, taskDetails, type IssueTarget } from './preview'

// D03 publish preview (IXD-21 – IXD-25). Everything is resolved locally by Rust: no network,
// no password prompt. Start sends the same request; Rust rebuilds the plan from the saved
// workspace and refuses it if the configuration changed since this preview.
const props = defineProps<{
  open: boolean
  /** The workspace to preview: the editor's draft when it has unsaved changes. */
  workspace: Workspace | null
  unsaved: boolean
  projectId: string
  pipelineId: string
  environmentId: string
}>()
const emit = defineEmits<{
  cancel: []
  jump: [target: IssueTarget, issues: Issue[]]
  started: [runId: string]
}>()

const toast = useToastStore()
const run = useRunStore()
const plan = ref<ResolvedPlan | null>(null)
const error = ref<string | null>(null)
const loading = ref(false)

const scope = ref<RunScope>('full')
/** Null until the operator changes the selection: then all bound servers are used. */
const serverIds = ref<string[] | null>(null)
/** Null until the operator types: then the resolved `version` variable is used. */
const version = ref<string | null>(null)
const typed = ref('')
const showDetails = ref(true)

const scopeOptions = [
  { value: 'full' as RunScope, label: '完整流程' },
  { value: 'localOnly' as RunScope, label: '仅本地命令' },
]

watch(
  () => props.open,
  (open) => {
    if (!open) return
    scope.value = 'full'
    serverIds.value = null
    version.value = null
    typed.value = ''
    plan.value = null
    startError.value = null
    key.value = crypto.randomUUID()
    void refresh()
  },
)

function request(): PlanRequest {
  return {
    projectId: props.projectId,
    pipelineId: props.pipelineId,
    environmentId: props.environmentId,
    scope: scope.value,
    serverIds: serverIds.value,
    version: version.value,
  }
}

let ticket = 0
async function refresh() {
  if (!props.workspace) return
  const mine = ++ticket
  loading.value = true
  error.value = null
  try {
    const result = await call('preview_run', { draft: props.workspace, request: request() })
    if (mine === ticket) plan.value = result
  } catch (e) {
    // An unsaved draft with a mistyped field cannot be planned; say which field.
    const err = toAppError(e)
    const issue = err.validation?.issues[0]
    if (mine === ticket)
      error.value = issue ? `${err.message}：${issue.path} ${issue.message}` : err.message
  } finally {
    if (mine === ticket) loading.value = false
  }
}

// IXD-24: changing version, scope or targets recomputes the plan and clears the typed name.
let timer: ReturnType<typeof setTimeout> | undefined
watch([scope, serverIds, version], () => {
  typed.value = ''
  key.value = crypto.randomUUID()
  clearTimeout(timer)
  timer = setTimeout(refresh, 200)
})

const versionText = computed({
  get: () => version.value ?? plan.value?.version ?? '',
  set: (v: string) => (version.value = v),
})

function toggleServer(id: string, on: boolean) {
  const current = serverIds.value ?? plan.value?.targets.map((t) => t.serverId) ?? []
  serverIds.value = on ? [...current.filter((x) => x !== id), id] : current.filter((x) => x !== id)
}

const serverFirst = computed(() => plan.value?.executionOrder === 'serverFirst')
const groups = computed(() => groupTasks(plan.value?.tasks ?? [], serverFirst.value))
const issues = computed(() => plan.value?.issues ?? [])
const fieldIssue = (field: 'version' | 'serverIds') =>
  issues.value.find((i) => i.path === field || i.path.startsWith(`${field}[`))?.message ?? null

/** Step issues by step id, to mark the affected tasks. */
const issueSteps = computed(
  () =>
    new Set(issues.value.filter((i) => i.object?.kind === 'step').map((i) => i.object?.id ?? '')),
)

const { programPath } = useShellPrograms()

const ORDER = { stepFirst: '步骤优先', serverFirst: '服务器优先' }

const details = (task: PlannedTask) => taskDetails(task, programPath)

async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text)
    toast.show('已复制')
  } catch {
    toast.show('复制失败')
  }
}

const confirmId = useId()
const body = ref<HTMLElement | null>(null)
async function onIssue(issue: Issue) {
  const target = issueTarget(issue)
  if (target.kind === 'dialog') {
    await nextTick()
    const el =
      target.field === 'version'
        ? body.value?.querySelector<HTMLElement>('.version input')
        : body.value?.querySelector<HTMLElement>('.targets input')
    el?.focus()
    return
  }
  emit('jump', target, issues.value)
}

const startReason = computed(() => {
  if (!plan.value || loading.value) return '正在生成预览'
  if (props.unsaved) return '未保存预览不能开始，请先保存'
  if (plan.value.issues.length) return `还有 ${plan.value.issues.length} 个问题需要处理`
  if (!plan.value.tasks.length) return '没有可执行的任务'
  if (plan.value.protected && typed.value !== plan.value.environmentName) {
    return `请键入“${plan.value.environmentName}”以确认`
  }
  if (run.activeId) return '已有运行正在进行，同一时间只能有一个运行'
  return null
})

// IXD-25: one key per previewed plan, so a repeated click returns the run already started.
const key = ref('')
const starting = ref(false)
const startError = ref<string | null>(null)
async function start() {
  if (!plan.value || starting.value) return
  starting.value = true
  startError.value = null
  try {
    const id = await run.start({
      request: request(),
      revision: plan.value.revision,
      confirmation: plan.value.protected ? typed.value : null,
      key: key.value,
    })
    emit('started', id)
  } catch (e) {
    const err = toAppError(e)
    if (err.kind === 'internal') {
      // The outcome is unknown: look for the run instead of starting again.
      const id = await run.checkActive().catch(() => null)
      if (id) {
        toast.show('运行已创建')
        emit('started', id)
        return
      }
    }
    startError.value = `运行未创建：${err.message}`
  } finally {
    starting.value = false
  }
}
</script>

<template>
  <UiDialog
    :open="open"
    title="发布预览"
    width="preview"
    :cancellable="!starting"
    @cancel="emit('cancel')"
  >
    <div ref="body" class="preview">
      <UiNotice v-if="error" tone="failure">{{ error }}</UiNotice>
      <UiNotice v-if="run.activeId" tone="warning">
        已有运行正在进行。
        <RouterLink :to="{ name: 'run', params: { runId: run.activeId } }" @click="emit('cancel')">
          查看运行
        </RouterLink>
      </UiNotice>
      <template v-if="plan">
        <header class="summary">
          <UiTag v-if="unsaved" tone="warning">未保存预览</UiTag>
          <dl class="facts">
            <div>
              <dt>项目</dt>
              <dd>{{ plan.projectName }}</dd>
            </div>
            <div>
              <dt>流程</dt>
              <dd>{{ plan.pipelineName }}</dd>
            </div>
            <div>
              <dt>环境</dt>
              <dd>
                {{ plan.environmentName }}
                <UiTag v-if="plan.protected" tone="warning" :icon="Shield">保护环境</UiTag>
              </dd>
            </div>
            <div>
              <dt>执行顺序</dt>
              <dd>{{ ORDER[plan.executionOrder] }}</dd>
            </div>
            <div>
              <dt>配置版本</dt>
              <dd class="num">{{ plan.revision }}</dd>
            </div>
          </dl>
        </header>

        <div class="params">
          <div class="scope">
            <span class="label">范围</span>
            <UiSegmented v-model="scope" :options="scopeOptions" label="范围" />
          </div>
          <UiField
            v-slot="{ id, describedBy, invalid }"
            class="version"
            label="版本"
            hint="默认取 version 变量；修改只影响本次运行"
            :error="fieldIssue('version')"
            required
          >
            <UiTextInput
              :id="id"
              v-model="versionText"
              mono
              :invalid="invalid"
              :described-by="describedBy"
            />
          </UiField>
        </div>

        <fieldset v-if="scope === 'full'" class="targets">
          <legend class="label">
            目标服务器（{{ plan.targets.filter((t) => t.selected).length }}）
          </legend>
          <p v-if="!plan.targets.length" class="muted">当前环境没有绑定目标服务器。</p>
          <UiCheckbox
            v-for="t in plan.targets"
            :key="t.serverId"
            :model-value="t.selected"
            :label="`${t.name}（${t.user}@${hostPort(t.host, t.port)}）`"
            @update:model-value="toggleServer(t.serverId, $event)"
          />
          <p class="muted">只能取消已绑定的服务器，修改只影响本次运行。</p>
          <p v-if="fieldIssue('serverIds')" class="field-error">{{ fieldIssue('serverIds') }}</p>
        </fieldset>
        <p v-else class="muted">仅执行本地命令，不连接服务器。</p>

        <section v-if="issues.length" class="issues" aria-label="预检问题">
          <h3 class="section-title">
            <CircleAlert :size="16" :stroke-width="1.75" aria-hidden="true" />
            {{ issues.length }} 个问题，修复后才能开始
          </h3>
          <ul>
            <li v-for="issue in issues" :key="issue.path + issue.message">
              <button type="button" class="issue" @click="onIssue(issue)">
                {{ issue.message }}
              </button>
            </li>
          </ul>
        </section>

        <section class="tasks" aria-label="任务">
          <div class="tasks-head">
            <h3 class="section-title">任务（{{ plan.tasks.length }}）</h3>
            <span class="toggle">
              <UiSwitch id="preview-details" v-model="showDetails" />
              <label for="preview-details">显示命令</label>
            </span>
          </div>
          <div v-for="group in groups" :key="group.key" class="group">
            <h4 class="group-title" :class="{ 'group-title--server': group.server }">
              {{ group.title }}
            </h4>
            <ol class="task-list">
              <li
                v-for="task in group.tasks"
                :key="task.seq"
                class="task"
                :class="{ 'task--issue': issueSteps.has(task.stepId) }"
              >
                <span class="seq num">{{ task.seq + 1 }}</span>
                <div class="task-main">
                  <div class="task-head">
                    <strong>{{ task.stepName }}</strong>
                    <span class="target">{{ task.server ? `→ ${task.server.name}` : '本地' }}</span>
                    <CircleAlert
                      v-if="issueSteps.has(task.stepId)"
                      class="task-issue"
                      :size="14"
                      :stroke-width="1.75"
                      aria-label="有问题"
                    />
                  </div>
                  <div v-if="showDetails" class="task-command">
                    <code>{{ taskCommand(task.action).text }}</code>
                    <UiIconButton
                      v-if="!taskCommand(task.action).sensitive"
                      class="copy"
                      tonal
                      :icon="Copy"
                      label="复制"
                      @click="copy(taskCommand(task.action).text)"
                    />
                  </div>
                  <p class="task-meta">{{ details(task).join(' · ') }}</p>
                </div>
              </li>
            </ol>
          </div>
        </section>

        <div v-if="plan.protected" class="confirm">
          <label :for="confirmId">
            键入 <strong class="env-name">{{ plan.environmentName }}</strong> 以确认发布到保护环境
          </label>
          <UiTextInput :id="confirmId" v-model="typed" mono />
        </div>
      </template>
      <p v-else-if="!error" class="muted">正在生成预览…</p>
    </div>

    <template #footer>
      <p v-if="startError" class="start-error" role="alert">{{ startError }}</p>
      <UiButton :disabled="starting" @click="emit('cancel')">取消</UiButton>
      <UiButton variant="primary" :busy="starting" :disabled-reason="startReason" @click="start">
        开始
      </UiButton>
    </template>
  </UiDialog>
</template>

<style scoped>
.start-error {
  flex: 1;
  margin: 0;
  color: var(--state-failure);
  font: var(--font-caption);
}

.preview {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.summary {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}

.facts {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2) var(--space-5);
  margin: 0;
}

.facts dt {
  color: var(--text-secondary);
  font: var(--font-caption);
}

.facts dd {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  font-weight: 600;
}

.params {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: var(--space-5);
  align-items: start;
}

.scope {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.label {
  font: var(--font-label);
}

.targets {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  border: 0;
}

.muted {
  margin: 0;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.field-error {
  margin: 0;
  color: var(--state-failure);
  font: var(--font-caption);
}

.issues {
  padding: var(--space-3);
  border-radius: var(--radius-control);
  background: var(--state-failure-bg);
}

.issues ul {
  margin: var(--space-2) 0 0;
  padding-left: var(--space-5);
}

.issue {
  padding: 0;
  border: 0;
  background: none;
  color: var(--state-failure);
  font: var(--font-body);
  text-align: left;
  text-decoration: underline;
  cursor: pointer;
}

.section-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  font: var(--font-section);
}

.tasks-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: var(--space-2);
}

.toggle {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
}

.group + .group {
  margin-top: var(--space-3);
}

.group-title {
  margin: 0 0 var(--space-1);
  color: var(--text-secondary);
  font: var(--font-label);
}

.group-title--server {
  color: var(--text-primary);
}

.task-list {
  margin: 0;
  padding: 0;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-control);
  list-style: none;
}

.task {
  display: flex;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--border-subtle);
}

.task:last-child {
  border-bottom: 0;
}

.task--issue {
  box-shadow: inset 3px 0 0 var(--state-failure);
}

.seq {
  width: 24px;
  color: var(--text-secondary);
  text-align: right;
}

.task-main {
  flex: 1;
  min-width: 0;
}

.task-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.target {
  color: var(--text-secondary);
}

.task-issue {
  color: var(--state-failure);
}

.task-command {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  margin-top: var(--space-1);
}

/* Tinted like other field-side actions, at the 32 px size of an inline button. */
.task-command .copy {
  width: 32px;
  height: 32px;
}

.task-command code {
  flex: 1;
  min-width: 0;
  font: var(--font-code);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.task-meta {
  margin: var(--space-1) 0 0;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.confirm {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.env-name {
  font: var(--font-code);
  font-weight: 600;
}
</style>

<script setup lang="ts">
import { CircleAlert, Copy, History, Shield } from '@lucide/vue'
import { computed, nextTick, ref, useId, watch } from 'vue'

import { dateTime } from '@/components/run/runState'
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
import { useT } from '@/i18n'
import { issueText, messageText } from '@/i18n/message'
import {
  call,
  toAppError,
  type Issue,
  type Message,
  type PlanRequest,
  type PlannedTask,
  type ResolvedPlan,
  type RunScope,
  type Workspace,
} from '@/ipc'
import { useShellPrograms } from '@/composables/shellPrograms'
import type { RerunSource } from '@/stores/preview'
import { useRunStore } from '@/stores/run'
import { useToastStore } from '@/stores/toast'

import {
  groupTasks,
  issueTarget,
  taskCommand,
  taskDetails,
  taskNotes,
  type IssueTarget,
} from './preview'

const t = useT()

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
  /** "Run again": also list what changed since this run (IXD-34, FLOW-07). */
  compareRun?: RerunSource | null
}>()
const emit = defineEmits<{
  cancel: []
  jump: [target: IssueTarget, issues: Issue[]]
  started: [runId: string]
}>()

const toast = useToastStore()
const run = useRunStore()
const plan = ref<ResolvedPlan | null>(null)
/** Differences from the compared run, once known. */
const changes = ref<Message[] | null>(null)
const error = ref<string | null>(null)
const loading = ref(false)

const scope = ref<RunScope>('full')
/** Null until the operator changes the selection: then all bound servers are used. */
const serverIds = ref<string[] | null>(null)
/** Null until the operator types: then the resolved `version` variable is used. */
const version = ref<string | null>(null)
const typed = ref('')
const showDetails = ref(true)

const scopeOptions = computed(() => [
  { value: 'full' as RunScope, label: t('preview.scopeFull') },
  { value: 'localOnly' as RunScope, label: t('preview.scopeLocal') },
])

watch(
  () => props.open,
  (open) => {
    if (!open) return
    scope.value = 'full'
    serverIds.value = null
    version.value = null
    typed.value = ''
    plan.value = null
    changes.value = null
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
    const diff = props.compareRun
      ? await call('plan_changes', { runId: props.compareRun.runId, plan: result })
      : null
    if (mine === ticket) {
      plan.value = result
      changes.value = diff
    }
  } catch (e) {
    // An unsaved draft with a mistyped field cannot be planned; say which field.
    const err = toAppError(e)
    const issue = err.validation?.issues[0]
    if (mine === ticket)
      error.value = issue
        ? t('preview.fieldError', {
            message: messageText(err.message),
            path: issue.path,
            detail: messageText(issue.message),
          })
        : messageText(err.message)
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
  issueText(issues.value.find((i) => i.path === field || i.path.startsWith(`${field}[`)))

/** Step issues by step id, to mark the affected tasks. */
const issueSteps = computed(
  () =>
    new Set(issues.value.filter((i) => i.object?.kind === 'step').map((i) => i.object?.id ?? '')),
)

const { programPath } = useShellPrograms()

const details = (task: PlannedTask) => taskDetails(task, programPath)

async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text)
    toast.show(t('common.copied'))
  } catch {
    toast.show(t('common.copyFailed'))
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
  if (!plan.value || loading.value) return t('preview.reason.loading')
  if (props.unsaved) return t('preview.reason.unsaved')
  if (plan.value.issues.length) return t('preview.reason.issues', { n: plan.value.issues.length })
  if (!plan.value.tasks.length) return t('preview.reason.noTasks')
  if (plan.value.protected && typed.value !== plan.value.environmentName) {
    return t('preview.reason.typeName', { name: plan.value.environmentName })
  }
  if (run.activeId) return t('preview.reason.active')
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
        toast.show(t('preview.runCreated'))
        emit('started', id)
        return
      }
    }
    startError.value = t('preview.runNotCreated', { message: messageText(err.message) })
  } finally {
    starting.value = false
  }
}
</script>

<template>
  <UiDialog
    :open="open"
    :title="t('preview.title')"
    width="preview"
    :cancellable="!starting"
    @cancel="emit('cancel')"
  >
    <div ref="body" class="preview">
      <UiNotice v-if="error" tone="failure">{{ error }}</UiNotice>
      <UiNotice v-if="run.activeId" tone="warning">
        {{ t('preview.activeRun') }}
        <RouterLink :to="{ name: 'run', params: { runId: run.activeId } }" @click="emit('cancel')">
          {{ t('preview.viewRun') }}
        </RouterLink>
      </UiNotice>
      <template v-if="plan">
        <header class="summary">
          <UiTag v-if="unsaved" tone="warning">{{ t('preview.unsaved') }}</UiTag>
          <dl class="facts">
            <div>
              <dt>{{ t('common.project') }}</dt>
              <dd>{{ plan.projectName }}</dd>
            </div>
            <div>
              <dt>{{ t('common.pipeline') }}</dt>
              <dd>{{ plan.pipelineName }}</dd>
            </div>
            <div>
              <dt>{{ t('common.environment') }}</dt>
              <dd>
                {{ plan.environmentName }}
                <UiTag v-if="plan.protected" tone="warning" :icon="Shield">{{
                  t('common.protectedEnv')
                }}</UiTag>
              </dd>
            </div>
            <div>
              <dt>{{ t('preview.order') }}</dt>
              <dd>{{ t(`pipelines.settings.${plan.executionOrder}`) }}</dd>
            </div>
            <div>
              <dt>{{ t('preview.revision') }}</dt>
              <dd class="num">{{ plan.revision }}</dd>
            </div>
          </dl>
        </header>

        <section
          v-if="compareRun && changes"
          class="changes"
          :aria-label="t('preview.rerun.label')"
        >
          <h3 class="changes-title">
            <History :size="16" :stroke-width="1.75" aria-hidden="true" />
            {{ t('preview.rerun.title', { time: dateTime(compareRun.createdAtMs) }) }}
          </h3>
          <ul v-if="changes.length" class="changes-list">
            <li v-for="(change, i) in changes" :key="i">
              <!-- Text changes show both values in full: commands can span lines. -->
              <template v-if="change.code === 'rerun.stepText'">
                {{
                  t('preview.rerun.field', {
                    step: change.params.step,
                    label: messageText(change.params.label),
                  })
                }}
                <dl class="change-values">
                  <dt>{{ t('preview.rerun.before') }}</dt>
                  <dd>
                    <code>{{ change.params.before || t('preview.rerun.empty') }}</code>
                  </dd>
                  <dt>{{ t('preview.rerun.after') }}</dt>
                  <dd>
                    <code>{{ change.params.after || t('preview.rerun.empty') }}</code>
                  </dd>
                </dl>
              </template>
              <template v-else>{{ messageText(change) }}</template>
            </li>
          </ul>
          <p v-else class="muted">{{ t('preview.rerun.none') }}</p>
        </section>

        <div class="params">
          <div class="scope">
            <span class="label">{{ t('preview.scope') }}</span>
            <UiSegmented v-model="scope" :options="scopeOptions" :label="t('preview.scope')" />
          </div>
          <UiField
            v-slot="{ id, describedBy, invalid }"
            class="version"
            :label="t('preview.version')"
            :hint="t('preview.versionHint')"
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
            {{ t('preview.targets', { n: plan.targets.filter((s) => s.selected).length }) }}
          </legend>
          <p v-if="!plan.targets.length" class="muted">{{ t('preview.noTargets') }}</p>
          <UiCheckbox
            v-for="s in plan.targets"
            :key="s.serverId"
            :model-value="s.selected"
            :label="
              t('preview.targetLabel', {
                name: s.name,
                address: `${s.user}@${hostPort(s.host, s.port)}`,
              })
            "
            @update:model-value="toggleServer(s.serverId, $event)"
          />
          <p class="muted">{{ t('preview.targetsHint') }}</p>
          <p v-if="fieldIssue('serverIds')" class="field-error">{{ fieldIssue('serverIds') }}</p>
        </fieldset>
        <p v-else class="muted">{{ t('preview.localOnly') }}</p>

        <section v-if="issues.length" class="issues" :aria-label="t('preview.issuesLabel')">
          <h3 class="section-title">
            <CircleAlert :size="16" :stroke-width="1.75" aria-hidden="true" />
            {{ t('preview.issuesTitle', { n: issues.length }) }}
          </h3>
          <ul>
            <li v-for="issue in issues" :key="issue.path + issue.code">
              <button type="button" class="issue" @click="onIssue(issue)">
                {{ messageText(issue.message) }}
              </button>
            </li>
          </ul>
        </section>

        <section class="tasks" :aria-label="t('preview.tasksLabel')">
          <div class="tasks-head">
            <h3 class="section-title">{{ t('preview.tasksTitle', { n: plan.tasks.length }) }}</h3>
            <span class="toggle">
              <UiSwitch id="preview-details" v-model="showDetails" />
              <label for="preview-details">{{ t('preview.showCommands') }}</label>
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
                    <span class="target">{{
                      task.server ? `→ ${task.server.name}` : t('common.local')
                    }}</span>
                    <CircleAlert
                      v-if="issueSteps.has(task.stepId)"
                      class="task-issue"
                      :size="14"
                      :stroke-width="1.75"
                      :aria-label="t('preview.hasIssue')"
                    />
                  </div>
                  <div v-if="showDetails" class="task-command">
                    <code>{{ taskCommand(task.action).text }}</code>
                    <UiIconButton
                      v-if="!taskCommand(task.action).sensitive"
                      class="copy"
                      tonal
                      :icon="Copy"
                      :label="t('common.copy')"
                      @click="copy(taskCommand(task.action).text)"
                    />
                  </div>
                  <p class="task-meta">{{ details(task).join(' · ') }}</p>
                  <p v-for="note in taskNotes(task.action)" :key="note" class="task-note">
                    {{ note }}
                  </p>
                </div>
              </li>
            </ol>
          </div>
        </section>

        <div v-if="plan.protected" class="confirm">
          <label :for="confirmId">
            <i18n-t keypath="preview.confirm">
              <template #name>
                <strong class="env-name">{{ plan.environmentName }}</strong>
              </template>
            </i18n-t>
          </label>
          <UiTextInput :id="confirmId" v-model="typed" mono />
        </div>
      </template>
      <p v-else-if="!error" class="muted">{{ t('preview.loadingLong') }}</p>
    </div>

    <template #footer>
      <p v-if="startError" class="start-error" role="alert">{{ startError }}</p>
      <UiButton :disabled="starting" @click="emit('cancel')">{{ t('common.cancel') }}</UiButton>
      <UiButton variant="primary" :busy="starting" :disabled-reason="startReason" @click="start">
        {{ t('common.start') }}
      </UiButton>
    </template>
  </UiDialog>
</template>

<style scoped>
.changes {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-surface);
  background: var(--surface-panel);
}

.changes-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font: var(--font-label);
}

.change-values {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: var(--space-1) var(--space-2);
  margin: var(--space-1) 0 var(--space-2);
  color: var(--text-secondary);
}

.change-values code {
  color: var(--text-primary);
}

.changes-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  margin: 0;
  padding-left: var(--space-5);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

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

.task-note {
  margin: 2px 0 0;
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

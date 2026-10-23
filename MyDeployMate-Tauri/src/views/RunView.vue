<script setup lang="ts">
import {
  ArrowLeft,
  CircleCheck,
  CircleDashed,
  CircleHelp,
  CircleMinus,
  CircleSlash,
  CircleX,
  KeyRound,
  LoaderCircle,
  OctagonX,
  PauseCircle,
  Shield,
} from '@lucide/vue'
import { computed, onBeforeUnmount, ref, shallowRef, watch, type Component } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import CredentialDialog from '@/components/credentials/CredentialDialog.vue'
import { credentialTitle } from '@/components/credentials/credentialLabels'
import EmptyState from '@/components/EmptyState.vue'
import PageLayout from '@/components/PageLayout.vue'
import { taskCommand, taskDetails } from '@/components/preview/preview'
import RunLogView from '@/components/run/RunLogView.vue'
import HostTrustDialog from '@/components/servers/HostTrustDialog.vue'
import {
  RUN_STATUS,
  TASK_STATUS,
  dateTime,
  elapsed,
  groupRunTasks,
  outcomeText,
  runFinished,
  skippedServerCount,
  taskFinished,
  taskLabel,
  type RunTask,
} from '@/components/run/runState'
import UiButton from '@/components/ui/UiButton.vue'
import UiConfirm from '@/components/ui/UiConfirm.vue'
import UiNotice from '@/components/ui/UiNotice.vue'
import UiTag from '@/components/ui/UiTag.vue'
import { useT } from '@/i18n'
import { toAppError, type TaskStatus } from '@/ipc'
import { useRunStore, type RunDetail } from '@/stores/run'
import { useToastStore } from '@/stores/toast'

const t = useT()

// P07 run detail (IXD-26 – IXD-30): summary with a cancel button that never scrolls away, the
// pending confirmation, the task list grouped as in the preview, the selected task and the log.
const route = useRoute()
const router = useRouter()
const runs = useRunStore()
const toast = useToastStore()

const runId = computed(() => String(route.params.runId ?? ''))
const detail = shallowRef<RunDetail | null>(null)
const loadError = ref<string | null>(null)
/** The task the operator clicked; until then the detail follows the run. */
const picked = ref<number | null>(null)

watch(
  runId,
  async (id) => {
    detail.value = runs.detail(id)
    loadError.value = null
    picked.value = null
    try {
      detail.value = await runs.open(id)
    } catch (e) {
      loadError.value = toAppError(e).message
    }
  },
  { immediate: true },
)

const run = computed(() => detail.value?.run ?? null)
const finished = computed(() => !!run.value && runFinished(run.value.status))

// Elapsed times tick while the run is active.
const now = ref(Date.now())
const timer = setInterval(() => (now.value = Date.now()), 1000)
onBeforeUnmount(() => clearInterval(timer))

const groups = computed(() =>
  detail.value ? groupRunTasks(detail.value.plan, detail.value.tasks) : [],
)
const items = computed(() => groups.value.flatMap((g) => g.tasks))

const selected = computed<RunTask | null>(() => {
  const list = items.value
  const by = (pred: (s: TaskStatus) => boolean) => list.find((t) => pred(t.task.status))
  return (
    list.find((t) => t.task.seq === picked.value) ??
    by((s) => ['running', 'waitingConfirmation', 'waitingCredential'].includes(s)) ??
    by((s) => s === 'failed' || s === 'unknown') ??
    [...list].reverse().find((t) => t.task.startedAtMs !== null) ??
    list[0] ??
    null
  )
})

const STATUS_ICON: Record<TaskStatus, Component> = {
  queued: CircleDashed,
  running: LoaderCircle,
  waitingConfirmation: PauseCircle,
  waitingCredential: KeyRound,
  succeeded: CircleCheck,
  failed: CircleX,
  cancelled: CircleSlash,
  skipped: CircleMinus,
  unknown: CircleHelp,
}

const done = computed(() => detail.value?.tasks.filter((t) => taskFinished(t.status)).length ?? 0)
const targetCount = computed(() =>
  run.value?.localOnly ? null : (detail.value?.plan.targets.filter((t) => t.selected).length ?? 0),
)
const skippedServers = computed(() => skippedServerCount(detail.value?.tasks ?? []))
const outcome = computed(() =>
  run.value && detail.value
    ? outcomeText(run.value.status, detail.value.tasks, detail.value.cancelReason)
    : null,
)
const firstFailure = computed(() =>
  items.value.find((t) => t.task.status === 'failed' || t.task.status === 'unknown'),
)
const unknownTasks = computed(() => items.value.filter((t) => t.task.status === 'unknown'))
const lastMessage = computed(() => {
  const d = detail.value
  if (!d) return null
  void d.logRevision
  const lines = d.lines.lines
  for (let i = lines.length - 1; i >= 0; i--) {
    const text = lines[i]?.text.trim()
    if (text) return text
  }
  return null
})

const OUTCOME_TONE = {
  succeeded: 'success',
  failed: 'failure',
  cancelled: 'warning',
  interrupted: 'unknown',
} as const

// IXD-28: the confirmation to-do, shown only when the run reaches it.
const confirming = computed(
  () => items.value.find((t) => t.task.status === 'waitingConfirmation') ?? null,
)
const confirmPrompt = computed(() => {
  const a = confirming.value?.plan.action
  return a?.type === 'confirm' ? a.prompt.display : ''
})
const before = computed(() => {
  const seq = confirming.value?.task.seq ?? 0
  const prior = detail.value?.tasks.filter((t) => t.seq < seq) ?? []
  return {
    succeeded: prior.filter((t) => t.status === 'succeeded').length,
    failed: prior.filter((t) => t.status === 'failed' || t.status === 'unknown'),
  }
})
const answering = ref(false)
async function proceed() {
  const d = detail.value
  const seq = confirming.value?.task.seq
  if (!d || seq === undefined || answering.value) return
  answering.value = true
  try {
    await runs.answer(d.runId, seq, true)
  } catch (e) {
    toast.show(toAppError(e).message)
  } finally {
    answering.value = false
  }
}

// IXD-31, IXD-32, IXD-19: a task waits for a secret (D04) or a first trust (D02). The dialog
// opens when the request arrives; declining it cancels the whole run.
const request = computed(() =>
  run.value?.status === 'waitingCredential' ? (detail.value?.request ?? null) : null,
)
const credentialPrompt = computed(() =>
  request.value?.prompt.type === 'credential' ? request.value.prompt : null,
)
const trustPrompt = computed(() =>
  request.value?.prompt.type === 'hostTrust' ? request.value.prompt : null,
)
const requestTitle = computed(() =>
  credentialPrompt.value ? credentialTitle(credentialPrompt.value) : '',
)
const requestOpen = ref(false)
const requestBusy = ref(false)
watch(
  () => request.value?.requestId,
  (id) => (requestOpen.value = id !== undefined),
  { immediate: true },
)
async function answerRequest(answer: (d: RunDetail, requestId: number) => Promise<unknown>) {
  const d = detail.value
  const r = request.value
  if (!d || !r || requestBusy.value) return
  requestBusy.value = true
  try {
    await answer(d, r.requestId)
    requestOpen.value = false
  } catch (e) {
    toast.show(toAppError(e).message)
  } finally {
    requestBusy.value = false
  }
}
const submitSecret = (secret: string | null, remember = false) =>
  answerRequest((d, id) => runs.answerSecret(d.runId, id, secret, remember))
const answerTrust = (trust: boolean) =>
  answerRequest((d, id) => runs.answerTrust(d.runId, id, trust))

// IXD-29: cancelling asks first, naming the current task and that nothing is undone.
const askCancel = ref(false)
const cancelling = ref(false)
const current = computed(() =>
  items.value.find((t) =>
    ['running', 'waitingConfirmation', 'waitingCredential'].includes(t.task.status),
  ),
)
async function cancel() {
  const d = detail.value
  if (!d) return
  cancelling.value = true
  try {
    await runs.cancel(d.runId)
    askCancel.value = false
  } catch (e) {
    toast.show(toAppError(e).message)
  } finally {
    cancelling.value = false
  }
}
const cancelReason = computed(() => {
  if (run.value?.status === 'cancelling') return t('runView.stopping')
  return null
})

function backToPipeline() {
  const r = run.value
  if (!r) return
  void router.push({
    name: 'pipeline',
    params: { projectId: r.projectId, pipelineId: r.pipelineId },
  })
}

/** The reason line, unless it only repeats the exit code shown above it. */
function extraReason(task: RunTask['task']) {
  // The backend's Chinese wording, until reasons become message codes (5b.3).
  return task.reason && task.reason !== `退出码 ${task.exitCode}` ? task.reason : null
}

function select(item: RunTask) {
  picked.value = item.task.seq
}
</script>

<template>
  <PageLayout :title="t('runView.title')">
    <template #actions>
      <UiButton v-if="finished" :icon="ArrowLeft" @click="backToPipeline">{{
        t('runView.back')
      }}</UiButton>
    </template>
    <UiNotice v-if="loadError" tone="failure">{{
      t('runView.loadFailed', { error: loadError })
    }}</UiNotice>
    <EmptyState v-else-if="!detail || !run" :title="t('runView.loading')" />
    <div v-else class="run-page">
      <header class="summary">
        <div class="summary-head">
          <UiTag
            :tone="RUN_STATUS[run.status].tone"
            :icon="finished ? undefined : LoaderCircle"
            :spin="!finished"
          >
            {{ trustPrompt ? t('runView.waitingTrust') : RUN_STATUS[run.status].label }}
          </UiTag>
          <h2 class="summary-title">{{ run.projectName }} · {{ run.pipelineName }}</h2>
          <span class="summary-env">
            {{ run.environmentName }}
            <UiTag v-if="detail.plan.protected" tone="warning" :icon="Shield">{{
              t('common.protectedEnv')
            }}</UiTag>
          </span>
          <span class="spacer" />
          <UiButton
            v-if="!finished"
            variant="danger"
            :icon="OctagonX"
            :busy="run.status === 'cancelling'"
            :disabled-reason="cancelReason"
            @click="askCancel = true"
          >
            {{ t('runView.cancel') }}
          </UiButton>
        </div>
        <dl class="facts">
          <div>
            <dt>{{ t('preview.order') }}</dt>
            <dd>{{ t(`pipelines.settings.${run.executionOrder}`) }}</dd>
          </div>
          <div>
            <dt>{{ t('runView.version') }}</dt>
            <dd class="mono">{{ run.version || '—' }}</dd>
          </div>
          <div>
            <dt>{{ t('runView.targets') }}</dt>
            <dd>
              {{
                targetCount === null
                  ? t('runView.localOnly')
                  : t('runView.targetCount', { n: targetCount })
              }}
            </dd>
          </div>
          <div>
            <dt>{{ t('runView.startedAt') }}</dt>
            <dd class="num">{{ dateTime(run.createdAtMs) }}</dd>
          </div>
          <div>
            <dt>{{ t('runView.elapsed') }}</dt>
            <dd class="num">{{ elapsed(run.createdAtMs, run.finishedAtMs, now) }}</dd>
          </div>
          <div>
            <dt>{{ t('runView.progress') }}</dt>
            <dd class="num">{{ done }}/{{ detail.tasks.length }}</dd>
          </div>
        </dl>
        <UiNotice v-if="detail.storageError" tone="failure">
          {{ t('runView.storageError', { error: detail.storageError }) }}
        </UiNotice>
        <UiNotice v-if="run.status === 'cancelling'" tone="warning">
          {{ t('runView.stoppingNotice')
          }}<span v-if="lastMessage" class="last">{{
            t('runView.lastOutput', { text: lastMessage })
          }}</span>
        </UiNotice>
        <UiNotice
          v-if="finished && outcome"
          :tone="OUTCOME_TONE[run.status as keyof typeof OUTCOME_TONE]"
        >
          {{ outcome }}
          <template v-if="skippedServers">{{
            t('runView.skippedServers', { n: skippedServers })
          }}</template>
          <button
            v-if="firstFailure && run.status === 'failed'"
            type="button"
            class="link"
            @click="select(firstFailure)"
          >
            {{ t('runView.viewFailed') }}
          </button>
          <ul v-if="run.status === 'interrupted' && unknownTasks.length" class="unknown">
            <li v-for="u in unknownTasks" :key="u.task.seq">
              {{ t('runView.unknownTask', { task: taskLabel(u.task) })
              }}<code>{{ taskCommand(u.plan.action).text }}</code>
            </li>
          </ul>
        </UiNotice>
      </header>

      <section v-if="confirming" class="todo" :aria-label="t('runView.confirmLabel')">
        <h3 class="todo-title">
          <PauseCircle :size="18" :stroke-width="1.75" aria-hidden="true" />
          {{ t('runView.confirmTitle', { n: confirming.task.seq + 1 }) }}
        </h3>
        <p class="todo-prompt">{{ confirmPrompt }}</p>
        <p class="todo-meta">
          {{
            t('runView.confirmMeta', {
              project: run.projectName,
              environment: run.environmentName,
              version: run.version || '—',
            })
          }}
          <template v-if="targetCount">
            {{
              t('runView.confirmTargets', {
                servers: detail.plan.targets
                  .filter((s) => s.selected)
                  .map((s) => s.name)
                  .join(t('common.listSeparator')),
              })
            }}
          </template>
        </p>
        <p class="todo-meta">
          {{ t('runView.before', { n: before.succeeded })
          }}<template v-if="before.failed.length">{{
            t('runView.beforeFailed', {
              n: before.failed.length,
              tasks: before.failed
                .map((f) => t('runView.taskRef', { n: f.seq + 1 }))
                .join(t('common.listSeparator')),
            })
          }}</template>
        </p>
        <div class="todo-actions">
          <UiButton variant="primary" :busy="answering" @click="proceed">{{
            t('runView.proceed')
          }}</UiButton>
          <UiButton variant="danger" :disabled="answering" @click="askCancel = true">
            {{ t('runView.abort') }}
          </UiButton>
        </div>
      </section>

      <section v-if="request" class="todo" :aria-label="t('runView.inputLabel')">
        <h3 class="todo-title">
          <KeyRound :size="18" :stroke-width="1.75" aria-hidden="true" />
          {{
            t(trustPrompt ? 'runView.waitingHostKey' : 'runView.waitingSecret', {
              n: request.task + 1,
            })
          }}
        </h3>
        <p v-if="credentialPrompt" class="todo-prompt">{{ requestTitle }}</p>
        <p v-if="credentialPrompt" class="todo-meta">
          {{
            t('runView.credentialMeta', {
              server: credentialPrompt.serverName,
              address: credentialPrompt.address,
              user: credentialPrompt.user,
            })
          }}
        </p>
        <p v-else-if="trustPrompt" class="todo-meta">
          {{ t('runView.trustMeta', { server: trustPrompt.serverName }) }}
        </p>
        <div class="todo-actions">
          <UiButton variant="primary" @click="requestOpen = true">
            {{ trustPrompt ? t('runView.checkFingerprint') : t('runView.enterSecret') }}
          </UiButton>
          <UiButton variant="danger" :disabled="requestBusy" @click="askCancel = true">
            {{ t('runView.abort') }}
          </UiButton>
        </div>
      </section>

      <div class="body">
        <nav class="tasks" :aria-label="t('runView.tasks')">
          <div v-for="group in groups" :key="group.key" class="group">
            <h3 class="group-title" :class="{ 'group-title--server': group.server }">
              {{ group.title }}
              <UiTag v-if="group.skipped" tone="warning">{{ group.skipped }}</UiTag>
            </h3>
            <ol class="task-list">
              <li v-for="item in group.tasks" :key="item.task.seq">
                <button
                  type="button"
                  class="task"
                  :class="[
                    `task--${TASK_STATUS[item.task.status].tone}`,
                    { 'task--selected': selected?.task.seq === item.task.seq },
                  ]"
                  :aria-current="selected?.task.seq === item.task.seq ? 'true' : undefined"
                  @click="select(item)"
                >
                  <component
                    :is="STATUS_ICON[item.task.status]"
                    class="task-icon"
                    :class="{ spin: item.task.status === 'running' }"
                    :size="16"
                    :stroke-width="1.75"
                    :aria-label="TASK_STATUS[item.task.status].label"
                  />
                  <span class="seq num">{{ item.task.seq + 1 }}</span>
                  <span class="task-main">
                    <span class="task-head">
                      <span class="task-name">{{ item.task.stepName }}</span>
                      <span class="task-target">{{
                        item.task.serverName ? `→ ${item.task.serverName}` : t('common.local')
                      }}</span>
                    </span>
                    <span v-if="item.task.reason" class="task-reason">{{ item.task.reason }}</span>
                  </span>
                  <span class="task-time num">
                    {{ elapsed(item.task.startedAtMs, item.task.finishedAtMs, now) ?? '' }}
                  </span>
                </button>
              </li>
            </ol>
          </div>
        </nav>

        <div class="side">
          <section v-if="selected" class="detail" :aria-label="t('runView.detail')">
            <div class="detail-head">
              <h3 class="detail-title">
                {{
                  t('runView.detailTitle', {
                    n: selected.task.seq + 1,
                    name: selected.task.stepName,
                  })
                }}
              </h3>
              <UiTag :tone="TASK_STATUS[selected.task.status].tone">
                {{ TASK_STATUS[selected.task.status].label }}
              </UiTag>
              <span class="task-target">{{
                selected.task.serverName ? `→ ${selected.task.serverName}` : t('common.local')
              }}</span>
            </div>
            <code class="detail-command">{{ taskCommand(selected.plan.action).text }}</code>
            <p class="detail-meta">{{ taskDetails(selected.plan).join(' · ') }}</p>
            <p class="detail-meta">
              <template v-if="selected.task.startedAtMs !== null">
                {{
                  t('runView.timing', {
                    start: dateTime(selected.task.startedAtMs),
                    elapsed:
                      elapsed(selected.task.startedAtMs, selected.task.finishedAtMs, now) ?? '',
                  })
                }}
              </template>
              <template v-else>{{ t('runView.notStarted') }}</template>
              <template v-if="selected.task.exitCode !== null">
                ·
                <span :class="{ 'exit-bad': selected.task.exitCode !== 0 }">
                  {{ t('runView.exitCode', { code: selected.task.exitCode }) }}
                </span>
              </template>
            </p>
            <p
              v-if="extraReason(selected.task)"
              class="detail-reason"
              :class="{
                'detail-reason--bad': ['failed', 'unknown'].includes(selected.task.status),
              }"
            >
              {{ selected.task.reason }}
            </p>
          </section>
          <RunLogView :detail="detail" :selected="picked" />
        </div>
      </div>
    </div>

    <CredentialDialog
      :prompt="requestOpen ? credentialPrompt : null"
      :request-id="request?.requestId ?? null"
      :busy="requestBusy"
      @submit="submitSecret"
      @cancel="submitSecret(null)"
    />
    <HostTrustDialog
      :open="requestOpen && !!trustPrompt"
      mode="first"
      :server-name="trustPrompt?.serverName ?? ''"
      :user="trustPrompt?.user ?? ''"
      :endpoint="trustPrompt?.endpoint ?? null"
      :presented="trustPrompt?.presented ?? null"
      :busy="requestBusy"
      @trust="answerTrust(true)"
      @cancel="answerTrust(false)"
    />
    <UiConfirm
      :open="askCancel"
      :title="t('runView.cancelTitle')"
      :confirm-label="t('runView.abort')"
      :cancel-label="t('closeRun.back')"
      danger
      :busy="cancelling"
      @confirm="cancel"
      @cancel="askCancel = false"
    >
      <p v-if="current" class="cancel-text">
        {{ t('runView.current', { task: taskLabel(current.task) }) }}
      </p>
      <p class="cancel-text">
        {{ t('runView.cancelBody') }}
      </p>
    </UiConfirm>
  </PageLayout>
</template>

<style scoped>
.run-page {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  height: 100%;
  min-height: 560px;
}

.summary {
  display: flex;
  flex: none;
  flex-direction: column;
  gap: var(--space-2);
}

.summary-head {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.summary-title {
  margin: 0;
  overflow: hidden;
  font: var(--font-section);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.summary-env {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--text-secondary);
}

.spacer {
  flex: 1;
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
  margin: 0;
  font-weight: 600;
}

.mono {
  font-family: var(--font-mono);
}

.last {
  margin-left: var(--space-2);
  font-family: var(--font-mono);
}

.link {
  margin-left: var(--space-2);
  padding: 0;
  border: 0;
  background: none;
  color: inherit;
  font: inherit;
  text-decoration: underline;
  cursor: pointer;
}

.unknown {
  margin: var(--space-2) 0 0;
  padding-left: var(--space-5);
}

.unknown code {
  font: var(--font-code);
}

.todo {
  flex: none;
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--state-warning);
  border-radius: var(--radius-surface);
  background: var(--state-warning-bg);
}

.todo-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  color: var(--state-warning);
  font: var(--font-section);
}

.todo-prompt {
  margin: var(--space-2) 0;
  font-weight: 600;
  white-space: pre-wrap;
}

.todo-meta {
  margin: 0;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.todo-actions {
  display: flex;
  gap: var(--space-2);
  margin-top: var(--space-3);
}

.body {
  display: grid;
  flex: 1;
  grid-template-columns: minmax(280px, 2fr) minmax(420px, 5fr);
  gap: var(--space-4);
  min-height: 0;
}

.tasks {
  min-height: 0;
  overflow: auto;
}

.group + .group {
  margin-top: var(--space-3);
}

.group-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
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

.task-list li + li {
  border-top: 1px solid var(--border-subtle);
}

.task {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  width: 100%;
  padding: var(--space-2) var(--space-3);
  border: 0;
  background: none;
  color: var(--text-primary);
  font: var(--font-body);
  text-align: left;
  cursor: pointer;
}

.task:hover {
  background: var(--surface-hover);
}

.task--selected {
  background: var(--surface-selected);
  box-shadow: inset 3px 0 0 var(--action-primary);
}

.task-icon {
  flex: none;
  margin-top: 3px;
  color: var(--text-secondary);
}

.task--success .task-icon {
  color: var(--state-success);
}

.task--failure .task-icon,
.task--failure .task-name {
  color: var(--state-failure);
}

.task--running .task-icon {
  color: var(--state-running);
}

.task--warning .task-icon {
  color: var(--state-warning);
}

.task--unknown .task-icon {
  color: var(--state-unknown);
}

.seq {
  flex: none;
  width: 20px;
  color: var(--text-secondary);
  text-align: right;
}

.task-main {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
}

.task-head {
  display: flex;
  gap: var(--space-2);
  min-width: 0;
}

.task-name {
  overflow: hidden;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.task-target {
  flex: none;
  color: var(--text-secondary);
}

.task-reason {
  color: var(--text-secondary);
  font: var(--font-caption);
}

.task-time {
  flex: none;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.side {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
  min-height: 0;
}

.detail {
  flex: none;
  padding: var(--space-3);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-surface);
}

.detail-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.detail-title {
  margin: 0;
  font: var(--font-label);
}

.detail-command {
  display: block;
  max-height: 88px;
  margin-top: var(--space-2);
  overflow: auto;
  font: var(--font-code);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.detail-meta {
  margin: var(--space-1) 0 0;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.exit-bad {
  color: var(--state-failure);
  font-weight: 600;
}

.detail-reason {
  margin: var(--space-1) 0 0;
  color: var(--text-secondary);
  font: var(--font-caption);
}

.detail-reason--bad {
  color: var(--state-failure);
}

.cancel-text {
  margin: 0;
}
</style>

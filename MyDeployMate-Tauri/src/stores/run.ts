import { Channel } from '@tauri-apps/api/core'
import { defineStore } from 'pinia'
import { computed, markRaw, reactive, ref, shallowRef } from 'vue'

import { RunLogLines } from '@/components/run/runLog'
import { runFinished, taskFinished } from '@/components/run/runState'
import {
  call,
  type Message,
  type ResolvedPlan,
  type Run,
  type RunId,
  type RunRequest,
  type RunSnapshot,
  type RunUpdate,
  type StartRunRequest,
  type Task,
  type UploadProgress,
} from '@/ipc'

/** What the shell shows about the single active run (SRS-1, IXD-02). */
export interface ActiveRunSummary {
  runId: string
  projectId: string
  projectName: string
  pipelineName: string
  environmentName: string
  /** Finished tasks over all tasks, for example 3/7 (UI-25). */
  done: number
  total: number
  waiting: 'confirm' | 'credential' | 'trust' | null
  stopping: boolean
}

/** A run as the run page shows it: the frozen plan, the live records and the log. */
export interface RunDetail {
  runId: RunId
  plan: ResolvedPlan
  run: Run
  tasks: Task[]
  cancelReason: Message | null
  lines: RunLogLines
  /** Bumped whenever `lines` changes, which is not tracked deeply. */
  logRevision: number
  damagedLines: number
  /** Updates still arrive; false once the run has ended or for a run opened after it ended. */
  live: boolean
  storageError: Message | null
  /** The open D02 or D04 request while the run waits for the operator. */
  request: RunRequest | null
  /** Latest progress of running upload tasks by task number; shown only, never stored. */
  progress: Record<number, UploadProgress>
}

/**
 * The active run and the run page's data. Opening a run reads what is recorded and attaches a
 * channel for later updates (`open_run`); a run started here is opened right away.
 */
export const useRunStore = defineStore('run', () => {
  /** The run in progress, known even before its details are loaded. */
  const activeId = ref<RunId | null>(null)
  /** The active run's details, kept while other pages are shown. */
  const live = shallowRef<RunDetail | null>(null)
  /** Another run opened on the run page (for example one that has just ended). */
  const viewed = shallowRef<RunDetail | null>(null)

  const active = computed<ActiveRunSummary | null>(() => {
    const d = live.value
    if (!d || !activeId.value || d.runId !== activeId.value || runFinished(d.run.status)) {
      return null
    }
    const status = d.run.status
    return {
      runId: d.runId,
      projectId: d.run.projectId,
      projectName: d.run.projectName,
      pipelineName: d.run.pipelineName,
      environmentName: d.run.environmentName,
      done: d.tasks.filter((t) => taskFinished(t.status)).length,
      total: d.tasks.length,
      waiting:
        status === 'waitingConfirmation'
          ? 'confirm'
          : status === 'waitingCredential'
            ? d.request?.prompt.type === 'hostTrust'
              ? 'trust'
              : 'credential'
            : null,
      stopping: status === 'cancelling',
    }
  })

  function detail(runId: RunId): RunDetail | null {
    if (live.value?.runId === runId) return live.value
    if (viewed.value?.runId === runId) return viewed.value
    return null
  }

  function apply(d: RunDetail, update: RunUpdate) {
    switch (update.type) {
      case 'state':
        d.run = update.run
        d.tasks = update.tasks
        d.cancelReason = update.cancelReason
        if (update.run.status !== 'waitingCredential') d.request = null
        if (runFinished(update.run.status)) {
          d.live = false
          if (activeId.value === d.runId) activeId.value = null
        }
        break
      case 'logs':
        if (d.lines.add(update.entries)) d.logRevision++
        break
      case 'request':
        d.request = { requestId: update.requestId, task: update.task, prompt: update.prompt }
        break
      case 'storageFailed':
        d.storageError = update.message
        break
      case 'progress':
        d.progress = { ...d.progress, [update.task]: update.progress }
        break
    }
  }

  const opening = new Map<RunId, Promise<RunDetail>>()

  /** Load a run for the run page; the active run keeps receiving updates. */
  function open(runId: RunId): Promise<RunDetail> {
    const known = detail(runId)
    // A live run keeps itself up to date and a finished one no longer changes.
    if (known && (known.live || runFinished(known.run.status))) return Promise.resolve(known)
    const pending = opening.get(runId)
    if (pending) return pending
    const promise = load(runId).finally(() => opening.delete(runId))
    opening.set(runId, promise)
    return promise
  }

  async function load(runId: RunId): Promise<RunDetail> {
    // Updates sent while the snapshot is on its way are newer than it; apply them after.
    let target: RunDetail | null = null
    const early: RunUpdate[] = []
    const channel = new Channel<RunUpdate>()
    channel.onmessage = (update) => (target ? apply(target, update) : early.push(update))
    const snapshot: RunSnapshot = await call('open_run', { runId, onUpdate: channel })
    const lines = markRaw(new RunLogLines())
    lines.add(snapshot.logs)
    const d = reactive<RunDetail>({
      runId,
      plan: markRaw(snapshot.plan),
      run: snapshot.run,
      tasks: snapshot.tasks,
      cancelReason: snapshot.cancelReason,
      lines,
      logRevision: 0,
      damagedLines: snapshot.damagedLines,
      live: snapshot.active && !runFinished(snapshot.run.status),
      storageError: null,
      request: snapshot.request,
      progress: {},
    }) as RunDetail
    target = d
    early.forEach((u) => apply(d, u))
    if (d.live) {
      activeId.value = runId
      live.value = d
    } else {
      if (activeId.value === runId) activeId.value = null
      if (live.value?.runId === runId) live.value = d
      else viewed.value = d
    }
    return d
  }

  /** Find the active run after the window was reloaded (IXD-02). */
  async function restore() {
    try {
      const id = await call('active_run')
      activeId.value = id
      if (id) await open(id)
    } catch {
      // Outside the desktop shell (tests, plain browser) there is no run to restore.
    }
  }

  /** Start a previewed run and load it. Throws `AppError` when it was not started. */
  async function start(request: StartRunRequest): Promise<RunId> {
    const id = await call('start_run', { start: request })
    activeId.value = id
    await open(id)
    return id
  }

  /** Ask the backend which run is active, after a start whose outcome is unknown (IXD-25). */
  async function checkActive(): Promise<RunId | null> {
    const id = await call('active_run')
    activeId.value = id
    return id
  }

  const cancel = (runId: RunId) => call('cancel_run', { runId })
  const answer = (runId: RunId, seq: number, proceed: boolean) =>
    call('answer_confirmation', { runId, seq, proceed })

  /** RUN-08: one line for the running command task `seq`, or `null` to end its input. */
  const sendInput = (runId: RunId, seq: number, line: string | null, hidden: boolean) =>
    call('send_run_input', { runId, seq, line, hidden })

  /** D04 during a run; `null` cancels the run (IXD-32). */
  const answerSecret = (
    runId: RunId,
    requestId: number,
    secret: string | null,
    remember: boolean,
  ) => call('answer_run_secret', { runId, requestId, secret, remember })
  /** D02 during a run; declining cancels the run. */
  const answerTrust = (runId: RunId, requestId: number, trust: boolean) =>
    call('answer_run_trust', { runId, requestId, trust })

  return {
    activeId,
    active,
    live,
    detail,
    open,
    restore,
    start,
    checkActive,
    cancel,
    answer,
    answerSecret,
    answerTrust,
    sendInput,
  }
})

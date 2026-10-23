import { translate, type MessageKey } from '@/i18n'
import { messageText } from '@/i18n/message'
import type { Message, PlannedTask, ResolvedPlan, RunStatus, Task, TaskStatus } from '@/ipc'
import type { Tone } from '@/components/ui/types'

import { groupTasks } from '@/components/preview/preview'

/** Status tones, with labels read in the current language each time. */
function statusMap<S extends RunStatus | TaskStatus>(
  prefix: 'run.status' | 'run.taskStatus',
  tones: Record<S, Tone>,
): Record<S, { label: string; tone: Tone }> {
  const out = {} as Record<S, { label: string; tone: Tone }>
  for (const status of Object.keys(tones) as S[]) {
    out[status] = {
      get label() {
        return translate(`${prefix}.${status}` as MessageKey)
      },
      tone: tones[status],
    }
  }
  return out
}

export const RUN_STATUS = statusMap<RunStatus>('run.status', {
  queued: 'neutral',
  running: 'running',
  waitingConfirmation: 'warning',
  waitingCredential: 'warning',
  cancelling: 'warning',
  succeeded: 'success',
  failed: 'failure',
  cancelled: 'neutral',
  interrupted: 'unknown',
})

export const TASK_STATUS = statusMap<TaskStatus>('run.taskStatus', {
  queued: 'neutral',
  running: 'running',
  waitingConfirmation: 'warning',
  waitingCredential: 'warning',
  succeeded: 'success',
  failed: 'failure',
  cancelled: 'neutral',
  skipped: 'neutral',
  unknown: 'unknown',
})

const TERMINAL_RUN: RunStatus[] = ['succeeded', 'failed', 'cancelled', 'interrupted']
const TERMINAL_TASK: TaskStatus[] = ['succeeded', 'failed', 'cancelled', 'skipped', 'unknown']

export const runFinished = (status: RunStatus) => TERMINAL_RUN.includes(status)
export const taskFinished = (status: TaskStatus) => TERMINAL_TASK.includes(status)

/** A planned task with its live record. */
export interface RunTask {
  plan: PlannedTask
  task: Task
}

export interface RunTaskGroup {
  key: string
  title: string
  server: boolean
  /** "已跳过（任务 N 失败）" when the skip-server policy stopped this server (IXD-30). */
  skipped: string | null
  tasks: RunTask[]
}

/** The task number whose failure made the skip-server policy stop this task's server. */
function skippedAfter(task: Task): number | null {
  return task.reason?.code === 'run.serverSkipped' ? task.reason.params.n : null
}

/** The task list grouped as in the preview (IXD-23, IXD-26). */
export function groupRunTasks(plan: ResolvedPlan, tasks: Task[]): RunTaskGroup[] {
  const bySeq = new Map(tasks.map((t) => [t.seq, t]))
  return groupTasks(plan.tasks, plan.executionOrder === 'serverFirst').map((g) => {
    const items = g.tasks.flatMap((p) => {
      const task = bySeq.get(p.seq)
      return task ? [{ plan: p, task }] : []
    })
    const n = g.server ? items.map((t) => skippedAfter(t.task)).find((x) => x !== null) : undefined
    return {
      ...g,
      skipped: n == null ? null : translate('run.skippedGroup', { n }),
      tasks: items,
    }
  })
}

/** How many servers the skip-server policy stopped (IXD-30). */
export function skippedServerCount(tasks: Task[]): number {
  const servers = new Set(tasks.filter((t) => skippedAfter(t) !== null).map((t) => t.serverId))
  return servers.size
}

export function taskLabel(task: Task): string {
  const n = task.seq + 1
  return task.serverName
    ? translate('run.taskLabelServer', { n, name: task.stepName, server: task.serverName })
    : translate('run.taskLabel', { n, name: task.stepName })
}

/** The line under the summary once the run has ended (IXD-26). */
export function outcomeText(status: RunStatus, tasks: Task[], cancelReason: Message | null) {
  const count = (s: TaskStatus) => tasks.filter((t) => t.status === s).length
  switch (status) {
    case 'succeeded':
      return translate('run.outcome.succeeded', { n: tasks.length })
    case 'failed': {
      const first = tasks.find((t) => t.status === 'failed' || t.status === 'unknown')
      const counts = translate('run.outcome.counts', {
        succeeded: count('succeeded'),
        skipped: count('skipped'),
      })
      if (!first) return counts
      const why = first.reason
        ? translate('run.outcome.why', { reason: messageText(first.reason) })
        : ''
      const key = first.status === 'unknown' ? 'run.outcome.unknown' : 'run.outcome.failed'
      return translate(key, { task: taskLabel(first), why, counts })
    }
    case 'cancelled': {
      const ran = tasks.filter((t) => t.startedAtMs !== null).length
      return translate('run.outcome.cancelled', {
        reason: cancelReason ? messageText(cancelReason) : translate('run.status.cancelled'),
        ran,
        rest: tasks.length - ran,
      })
    }
    case 'interrupted': {
      // Set by history recovery when the app quit mid-run.
      const appExited = tasks.some((t) => t.reason?.code === 'run.appExited')
      return translate(appExited ? 'run.outcome.appExited' : 'run.outcome.interrupted')
    }
    default:
      return null
  }
}

/** 860 ms → "0.9 秒"; 65 s → "1 分 05 秒"; 2 h → "2 小时 00 分". */
export function duration(ms: number): string {
  const s = Math.max(0, ms) / 1000
  if (s < 60) return translate('common.seconds', { n: s < 10 ? s.toFixed(1) : Math.floor(s) })
  const two = (n: number) => String(n).padStart(2, '0')
  const total = Math.floor(s)
  if (total < 3600) {
    return translate('run.duration.minutes', { m: Math.floor(total / 60), s: two(total % 60) })
  }
  return translate('run.duration.hours', {
    h: Math.floor(total / 3600),
    m: two(Math.floor((total % 3600) / 60)),
  })
}

/** Elapsed time of something started at `start`, still running when `end` is null. */
export function elapsed(start: number | null, end: number | null, now: number): string | null {
  if (start === null) return null
  return duration((end ?? now) - start)
}

export function dateTime(atMs: number): string {
  const d = new Date(atMs)
  const two = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}:${two(d.getMinutes())}:${two(d.getSeconds())}`
}

import type { PlannedTask, ResolvedPlan, RunStatus, Task, TaskStatus } from '@/ipc'
import type { Tone } from '@/components/ui/types'

import { groupTasks } from '@/components/preview/preview'

export const RUN_STATUS: Record<RunStatus, { label: string; tone: Tone }> = {
  queued: { label: '排队中', tone: 'neutral' },
  running: { label: '运行中', tone: 'running' },
  waitingConfirmation: { label: '等待确认', tone: 'warning' },
  waitingCredential: { label: '等待凭据', tone: 'warning' },
  cancelling: { label: '正在取消', tone: 'warning' },
  succeeded: { label: '成功', tone: 'success' },
  failed: { label: '失败', tone: 'failure' },
  cancelled: { label: '已取消', tone: 'neutral' },
  interrupted: { label: '已中断', tone: 'unknown' },
}

export const TASK_STATUS: Record<TaskStatus, { label: string; tone: Tone }> = {
  queued: { label: '待执行', tone: 'neutral' },
  running: { label: '运行中', tone: 'running' },
  waitingConfirmation: { label: '等待确认', tone: 'warning' },
  waitingCredential: { label: '等待凭据', tone: 'warning' },
  succeeded: { label: '成功', tone: 'success' },
  failed: { label: '失败', tone: 'failure' },
  cancelled: { label: '已取消', tone: 'neutral' },
  skipped: { label: '已跳过', tone: 'neutral' },
  unknown: { label: '结果未知', tone: 'unknown' },
}

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

const SKIPPED_SERVER = /^服务器 .+ 已跳过（(任务 \d+ 失败)）$/

/** The task list grouped as in the preview (IXD-23, IXD-26). */
export function groupRunTasks(plan: ResolvedPlan, tasks: Task[]): RunTaskGroup[] {
  const bySeq = new Map(tasks.map((t) => [t.seq, t]))
  return groupTasks(plan.tasks, plan.executionOrder === 'serverFirst').map((g) => {
    const items = g.tasks.flatMap((p) => {
      const task = bySeq.get(p.seq)
      return task ? [{ plan: p, task }] : []
    })
    const hit = g.server
      ? items.map((t) => SKIPPED_SERVER.exec(t.task.reason ?? '')).find((m) => m)
      : undefined
    return { ...g, skipped: hit ? `已跳过（${hit[1]}）` : null, tasks: items }
  })
}

/** How many servers the skip-server policy stopped (IXD-30). */
export function skippedServerCount(tasks: Task[]): number {
  const servers = new Set(
    tasks.filter((t) => SKIPPED_SERVER.test(t.reason ?? '')).map((t) => t.serverId),
  )
  return servers.size
}

export function taskLabel(task: Task): string {
  const target = task.serverName ? ` · ${task.serverName}` : ''
  return `任务 ${task.seq + 1}（${task.stepName}${target}）`
}

/** The line under the summary once the run has ended (IXD-26). */
export function outcomeText(status: RunStatus, tasks: Task[], cancelReason: string | null) {
  const count = (s: TaskStatus) => tasks.filter((t) => t.status === s).length
  switch (status) {
    case 'succeeded':
      return `全部 ${tasks.length} 个任务成功`
    case 'failed': {
      const first = tasks.find((t) => t.status === 'failed' || t.status === 'unknown')
      const why = first?.reason ? `：${first.reason}` : ''
      const others = `成功 ${count('succeeded')} 个，跳过 ${count('skipped')} 个`
      const what = first?.status === 'unknown' ? '结果未知' : '失败'
      return first ? `${taskLabel(first)}${what}${why}。${others}` : others
    }
    case 'cancelled': {
      const ran = tasks.filter((t) => t.startedAtMs !== null).length
      return `${cancelReason ?? '已取消'}。已执行 ${ran} 个任务，未执行 ${tasks.length - ran} 个；已完成的操作不会自动撤销`
    }
    case 'interrupted':
      return '应用在运行过程中退出，运行没有继续。结果未知的任务需要人工核验'
    default:
      return null
  }
}

/** 860 ms → "0.9 秒"; 65 s → "1 分 05 秒"; 2 h → "2 小时 00 分". */
export function duration(ms: number): string {
  const s = Math.max(0, ms) / 1000
  if (s < 60) return `${s < 10 ? s.toFixed(1) : Math.floor(s)} 秒`
  const two = (n: number) => String(n).padStart(2, '0')
  const total = Math.floor(s)
  if (total < 3600) return `${Math.floor(total / 60)} 分 ${two(total % 60)} 秒`
  return `${Math.floor(total / 3600)} 小时 ${two(Math.floor((total % 3600) / 60))} 分`
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

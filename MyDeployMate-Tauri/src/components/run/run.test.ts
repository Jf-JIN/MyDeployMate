import { describe, expect, it } from 'vitest'

import type { LogEntry, LogSource, PlannedTask, ResolvedPlan, Task, TaskStatus } from '@/ipc'

import { highlight, LIVE_LINES, RunLogLines } from './runLog'
import { duration, groupRunTasks, outcomeText, skippedServerCount } from './runState'

let n = 0
const entry = (text: string, source: LogSource = 'stdout', task = 0): LogEntry => ({
  n: n++,
  task,
  source,
  text,
  atMs: 1000,
})

describe('run log lines', () => {
  it('joins pieces of a line and keeps streams apart', () => {
    const log = new RunLogLines()
    log.add([entry('he'), entry('warn\n', 'stderr'), entry('llo\r\nnext'), entry(' line\n')])
    expect(log.lines.map((l) => [l.source, l.text])).toEqual([
      ['stdout', 'hello'],
      ['stderr', 'warn'],
      ['stdout', 'next line'],
    ])
  })

  it('keeps empty lines and drops entries already seen', () => {
    const log = new RunLogLines()
    const first = [entry('a\n\nb\n')]
    expect(log.add(first)).toBe(true)
    expect(log.add(first)).toBe(false)
    expect(log.lines.map((l) => l.text)).toEqual(['a', '', 'b'])
  })

  it('marks the truncation entry', () => {
    const log = new RunLogLines()
    log.add([{ ...entry('\n已达到上限\n', 'system'), truncated: true }])
    expect(log.lines.map((l) => [l.text, l.truncated])).toEqual([
      ['', true],
      ['已达到上限', true],
    ])
  })

  it('starts new lines after the marker', () => {
    const log = new RunLogLines()
    log.add([entry('kept'), { ...entry('上限\n', 'system'), truncated: true }])
    log.add([{ ...entry('live\n'), live: true }])
    expect(log.lines.map((l) => [l.text, l.live])).toEqual([
      ['kept', false],
      ['上限', false],
      ['live', true],
    ])
  })

  it('keeps only the newest live lines', () => {
    const log = new RunLogLines()
    log.add([entry('head\n'), { ...entry('上限\n', 'system'), truncated: true }])
    const live = (i: number) => ({ ...entry(`l${i}\n`), live: true })
    for (let i = 0; i < 2 * LIVE_LINES - 1; i++) log.add([live(i)])
    log.add([entry('note\n', 'system'), { ...entry('open'), live: true }])
    expect(log.trimmed).toBe(0)
    log.add([{ ...entry(' more\n'), live: true }, live(2 * LIVE_LINES)])
    expect(log.trimmed).toBe(LIVE_LINES + 1)
    expect(log.lines.length).toBe(2 + LIVE_LINES + 1)
    expect(log.lines.slice(0, 3).map((l) => l.text)).toEqual(['head', '上限', `l${LIVE_LINES + 1}`])
    // The open line moved with the rest and still took its end.
    expect(log.lines.slice(-3).map((l) => l.text)).toEqual([
      'note',
      'open more',
      `l${2 * LIVE_LINES}`,
    ])
    const ids = log.lines.map((l) => l.index)
    expect(ids.every((id, i) => i === 0 || id > (ids[i - 1] ?? -1))).toBe(true)
  })
})

describe('highlight', () => {
  it('marks every match without changing the text', () => {
    const parts = highlight('Error: error here', 'error')
    expect(parts.map((p) => p.text).join('')).toBe('Error: error here')
    expect(parts.filter((p) => p.hit).map((p) => p.text)).toEqual(['Error', 'error'])
  })

  it('returns the text whole without a query', () => {
    expect(highlight('<b>x</b>', '')).toEqual([{ text: '<b>x</b>', hit: false }])
  })
})

function planned(seq: number, server: string | null, segment: number): PlannedTask {
  return {
    seq,
    stepId: `s${seq}`,
    stepIndex: seq,
    stepName: `step ${seq}`,
    kind: server ? 'remote' : 'local',
    segment,
    server: server ? { id: server, name: server, host: server } : null,
    timeoutSec: 60,
    failurePolicy: 'skipServer',
    action: { type: 'confirm', prompt: { display: 'ok?', sensitive: false } },
  }
}

function task(seq: number, status: TaskStatus, server: string | null, reason?: string): Task {
  return {
    id: `t${seq}`,
    runId: 'r',
    seq,
    stepId: `s${seq}`,
    stepName: `step ${seq}`,
    kind: server ? 'remote' : 'local',
    serverId: server,
    serverName: server,
    status,
    exitCode: null,
    exitSignal: null,
    reason: reason ?? null,
    startedAtMs: status === 'queued' || status === 'skipped' ? null : 0,
    finishedAtMs: null,
  }
}

describe('run state', () => {
  const plan = {
    executionOrder: 'serverFirst',
    tasks: [planned(0, null, 0), planned(1, 'a', 1), planned(2, 'a', 1), planned(3, 'b', 1)],
  } as ResolvedPlan

  it('marks a server group stopped by the skip-server policy', () => {
    const tasks = [
      task(0, 'succeeded', null),
      task(1, 'failed', 'a', '退出码 1'),
      task(2, 'skipped', 'a', '服务器 a 已跳过（任务 2 失败）'),
      task(3, 'succeeded', 'b'),
    ]
    const groups = groupRunTasks(plan, tasks)
    expect(groups.map((g) => [g.title, g.skipped])).toEqual([
      ['步骤 1 · step 0', null],
      ['a', '已跳过（任务 2 失败）'],
      ['b', null],
    ])
    expect(skippedServerCount(tasks)).toBe(1)
    expect(outcomeText('failed', tasks, null)).toBe(
      '任务 2（step 1 · a）失败：退出码 1。成功 2 个，跳过 1 个',
    )
  })

  it('says what ran before a cancel', () => {
    const tasks = [task(0, 'succeeded', null), task(1, 'cancelled', 'a'), task(2, 'skipped', 'a')]
    expect(outcomeText('cancelled', tasks, '用户取消')).toBe(
      '用户取消。已执行 2 个任务，未执行 1 个；已完成的操作不会自动撤销',
    )
  })

  it('formats durations', () => {
    expect(duration(860)).toBe('0.9 秒')
    expect(duration(42_000)).toBe('42 秒')
    expect(duration(65_000)).toBe('1 分 05 秒')
    expect(duration(7_260_000)).toBe('2 小时 01 分')
  })
})

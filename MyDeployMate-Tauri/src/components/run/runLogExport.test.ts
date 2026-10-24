import { describe, expect, it } from 'vitest'

import type { PlannedTask, ResolvedPlan, Run } from '@/ipc'
import type { RunDetail } from '@/stores/run'

import { RunLogLines } from './runLog'
import { exportFileName, formatRunLog } from './runLogExport'

const at = new Date(2026, 9, 24, 5, 30, 9).getTime()

function detail(): RunDetail {
  const run = {
    id: 'run-1',
    projectName: 'account-api',
    pipelineName: '构建 并部署',
    environmentName: 'staging',
    version: 'v1.8.0',
    createdAtMs: at,
  } as Run
  const task = (seq: number, server: string | null) =>
    ({
      seq,
      stepName: seq === 0 ? 'build' : 'restart',
      server: server ? { id: 's', name: server, host: 'h' } : null,
    }) as PlannedTask
  const lines = new RunLogLines()
  lines.add([
    { n: 0, task: 0, source: 'stdout', text: 'ok\n', atMs: at },
    { n: 1, task: 1, source: 'stderr', text: 'warn\n', atMs: at + 1000, truncated: true },
    { n: 2, task: 1, source: 'stdout', text: 'live only\n', atMs: at + 2000, live: true },
  ])
  return {
    runId: 'run-1',
    run,
    plan: { tasks: [task(0, null), task(1, 'api-01')] } as ResolvedPlan,
    lines,
  } as RunDetail
}

describe('log export', () => {
  it('names the file after the run, safe for Windows', () => {
    expect(exportFileName(detail())).toBe('account-api_构建_并部署_20261024-053009.log')
  })

  it('keeps time and context on every saved line and leaves out live-only output', () => {
    const text = formatRunLog(detail())
    const lines = text.trimEnd().split('\r\n')
    expect(lines[0]).toContain('account-api')
    expect(lines[1]).toContain('run-1')
    expect(lines).toContain('2026-10-24 05:30:09 [#1 build @ 本地] [stdout] ok')
    expect(lines).toContain('2026-10-24 05:30:10 [#2 restart @ api-01] [stderr] warn')
    expect(text).not.toContain('live only')
    const marker = lines.findIndex((l) => l.includes('超过单次运行上限'))
    expect(lines[marker + 1]).toContain('warn')
  })
})

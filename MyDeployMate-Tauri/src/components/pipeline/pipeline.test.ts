import { describe, expect, it } from 'vitest'

import type { Pipeline, Server, Step } from '@/ipc'

import {
  clearSkipServer,
  effectivePrivilege,
  exampleSequence,
  insertExpression,
  moveStep,
  nextSelection,
  skipServerUsers,
} from './pipeline'

function step(id: string, failurePolicy: Step['failurePolicy'] = 'inherit'): Step {
  return {
    id,
    name: id,
    enabled: true,
    timeoutSec: 300,
    failurePolicy,
    spec: { type: 'confirm', prompt: 'ok' },
  }
}

function pipeline(): Pipeline {
  return {
    id: 'p',
    name: 'p',
    executionOrder: 'serverFirst',
    failurePolicy: 'skipServer',
    steps: [step('a'), step('b', 'skipServer'), step('c')],
  }
}

describe('pipeline helpers', () => {
  it('finds and clears skip-server users', () => {
    const p = pipeline()
    expect(skipServerUsers(p)).toEqual({ pipeline: true, steps: [p.steps[1]] })
    clearSkipServer(p)
    expect(p.failurePolicy).toBe('stop')
    expect(skipServerUsers(p)).toEqual({ pipeline: false, steps: [] })
  })

  it('moves steps within bounds and picks the next selection', () => {
    const steps = pipeline().steps
    expect(moveStep(steps, 0, 1)).toBe(1)
    expect(steps.map((s) => s.id)).toEqual(['b', 'a', 'c'])
    expect(moveStep(steps, 0, -1)).toBe(0)
    steps.splice(2, 1)
    expect(nextSelection(steps, 2)).toBe('a')
    expect(nextSelection([], 0)).toBeNull()
  })

  it('inserts at the cursor and replaces a selection', () => {
    expect(insertExpression('cd  && ls', 3, 3, '${deploy_dir}')).toEqual({
      value: 'cd ${deploy_dir} && ls',
      caret: 16,
    })
    expect(insertExpression('abc', 1, 2, 'X')).toEqual({ value: 'aXc', caret: 2 })
  })

  it('explains both execution orders', () => {
    expect(exampleSequence('stepFirst').slice(1, 3)).toEqual(['上传 → A', '上传 → B'])
    expect(exampleSequence('serverFirst').slice(1, 3)).toEqual(['上传 → A', '重启 → A'])
  })

  it('names the privilege each server gets', () => {
    const server = { privilege: { mode: 'sudoNonInteractive', runAs: 'root' } } as Server
    expect(effectivePrivilege({ mode: 'inherit' }, server)).toBe(
      'sudo 免交互，以 root 运行（服务器默认）',
    )
    expect(effectivePrivilege({ mode: 'none' }, server)).toBe('不提权')
  })
})

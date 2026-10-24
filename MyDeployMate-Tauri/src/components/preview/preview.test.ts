import { describe, expect, it } from 'vitest'

import type { Issue, PlannedTask } from '@/ipc'

import {
  editorField,
  groupTasks,
  issueTarget,
  taskCommand,
  taskDetails,
  taskNotes,
} from './preview'

const text = (display: string, sensitive = false) => ({ display, sensitive })

function task(seq: number, step: number, server: string | null): PlannedTask {
  return {
    seq,
    stepId: `s${step}`,
    stepIndex: step,
    stepName: `step ${step}`,
    kind: server ? 'remote' : 'local',
    segment: server ? 1 : step,
    server: server ? { id: server, name: server.toUpperCase(), host: server } : null,
    timeoutSec: 300,
    failurePolicy: 'stop',
    action: server
      ? {
          type: 'remote',
          command: { mode: 'script', script: text('restart') },
          cwd: null,
          privilege: { mode: 'none' },
          privilegeInherited: false,
          shellElevation: false,
        }
      : {
          type: 'local',
          command: text('build'),
          cwd: null,
          shell: 'powershell',
          wslDistro: null,
          outputEncoding: 'auto',
        },
  }
}

function issue(path: string, object: Issue['object'] = null): Issue {
  return {
    path,
    object,
    field: '',
    code: 'required',
    message: { code: 'text', params: { text: '' } },
  }
}

describe('preview', () => {
  it('groups server-first remote tasks by server and keeps numbering', () => {
    const tasks = [task(0, 0, null), task(1, 1, 'a'), task(2, 2, 'a'), task(3, 1, 'b')]
    const groups = groupTasks(tasks, true)
    expect(groups.map((g) => g.title)).toEqual(['步骤 1 · step 0', 'A', 'B'])
    expect(groups[1]?.tasks.map((t) => t.seq)).toEqual([1, 2])
  })

  it('groups step-first tasks by step', () => {
    const tasks = [task(0, 1, 'a'), task(1, 1, 'b')]
    tasks.forEach((t) => (t.segment = 0))
    expect(groupTasks(tasks, false).map((g) => g.tasks.length)).toEqual([2])
  })

  it('sends issues to where they can be fixed', () => {
    expect(issueTarget(issue('version'))).toEqual({ kind: 'dialog', field: 'version' })
    expect(issueTarget(issue('serverIds[1]'))).toEqual({ kind: 'dialog', field: 'serverIds' })
    expect(
      issueTarget(
        issue('projects[0].pipelines[0].steps[2].spec.command', { kind: 'step', id: 'x' }),
      ),
    ).toEqual({ kind: 'step', stepId: 'x' })
    expect(issueTarget(issue('projects[0].variables[1].value'))).toEqual({
      kind: 'route',
      name: 'project-variables',
    })
    expect(issueTarget(issue('servers[3].host'))).toEqual({ kind: 'route', name: 'servers' })
    expect(issueTarget(issue('globalVariables[0].name'))).toEqual({
      kind: 'route',
      name: 'settings',
    })
  })

  it('maps argument paths to the editor field', () => {
    expect(editorField('.spec.command.args[3]')).toBe('.spec.command.args')
  })

  it('marks commands with sensitive parts', () => {
    const t = task(0, 1, 'a')
    if (t.action.type === 'remote' && t.action.command.mode === 'script') {
      t.action.command.script = text('echo ******', true)
    }
    expect(taskCommand(t.action)).toEqual({ text: 'echo ******', sensitive: true })
  })

  it('names a fixed output encoding but not the automatic one', () => {
    const t = task(0, 0, null)
    expect(taskDetails(t).join(' · ')).not.toContain('输出编码')
    if (t.action.type === 'local') t.action.outputEncoding = 'gbk'
    expect(taskDetails(t)).toContain('输出编码 GBK')
  })

  it('shows where the privilege comes from and what sudo needs (IXD-33)', () => {
    const t = task(0, 0, 'a')
    if (t.action.type !== 'remote') throw new Error('remote task expected')
    expect(taskDetails(t)).toContain('不提权')
    expect(taskNotes(t.action)).toEqual([])
    t.action.privilege = { mode: 'sudoPassword', runAs: 'root' }
    t.action.privilegeInherited = true
    t.action.shellElevation = true
    expect(taskDetails(t)).toContain('sudo 输入密码 → root（继承服务器默认）')
    const notes = taskNotes(t.action)
    expect(notes).toHaveLength(2)
    expect(notes[0]).toContain('sudo sh -c')
    expect(notes[1]).toContain('只在服务器要求时询问密码')
    t.action.privilege = { mode: 'sudoNonInteractive', runAs: 'app' }
    t.action.shellElevation = false
    expect(taskNotes(t.action)).toEqual(['服务器未授予免交互权限时该任务失败，不会改为询问密码'])
    expect(
      taskNotes({
        type: 'upload',
        source: text('dist'),
        sourceKind: 'directory',
        target: text('/srv/www'),
        conflict: 'overwrite',
      })[0],
    ).toContain('不使用 sudo')
  })
})

import { shellLabel } from '@/composables/shellPrograms'
import type { Issue, PlannedTask, PrivilegePolicy, ResolvedCommand, Shell, TaskAction } from '@/ipc'

/** Where clicking a preflight issue leads (IXD-24). */
export type IssueTarget =
  | { kind: 'dialog'; field: 'version' | 'serverIds' }
  | { kind: 'step'; stepId: string }
  | { kind: 'pipeline' }
  | { kind: 'route'; name: string }

export function issueTarget(issue: Issue): IssueTarget {
  const p = issue.path
  if (p === 'version') return { kind: 'dialog', field: 'version' }
  if (p === 'serverIds' || p.startsWith('serverIds[')) return { kind: 'dialog', field: 'serverIds' }
  if (issue.object?.kind === 'step') return { kind: 'step', stepId: issue.object.id }
  if (p === 'steps' || /^projects\[\d+\]\.pipelines\[\d+\]/.test(p)) return { kind: 'pipeline' }
  if (/^projects\[\d+\]\.targets/.test(p)) return { kind: 'route', name: 'project-targets' }
  if (/^projects\[\d+\]\.(variables|overrides)/.test(p)) {
    return { kind: 'route', name: 'project-variables' }
  }
  if (p.startsWith('projects[')) return { kind: 'route', name: 'project-settings' }
  if (p.startsWith('servers[')) return { kind: 'route', name: 'servers' }
  return { kind: 'route', name: 'settings' }
}

/** Plan issue paths name each program argument; the editor has one field for all of them. */
export function editorField(path: string): string {
  return path.replace(/\.spec\.command\.args\[\d+\]$/, '.spec.command.args')
}

/** One heading and its tasks, in plan order (IXD-23). */
export interface TaskGroup {
  key: string
  title: string
  /** A server group inside a server-first segment. */
  server: boolean
  tasks: PlannedTask[]
}

/**
 * Step-first: a group per step. Server-first: local and confirm steps stand alone, and the
 * remote steps between them are grouped by server. Global numbering is kept either way.
 */
export function groupTasks(tasks: PlannedTask[], serverFirst: boolean): TaskGroup[] {
  const groups: TaskGroup[] = []
  for (const task of tasks) {
    const byServer = serverFirst && task.server
    const key = byServer ? `${task.segment}:${task.server?.id}` : `${task.segment}`
    const last = groups[groups.length - 1]
    if (last?.key === key) {
      last.tasks.push(task)
    } else {
      groups.push({
        key,
        title: byServer
          ? (task.server?.name ?? '')
          : `步骤 ${task.stepIndex + 1} · ${task.stepName}`,
        server: !!byServer,
        tasks: [task],
      })
    }
  }
  return groups
}

export function privilegeLabel(policy: PrivilegePolicy): string {
  switch (policy.mode) {
    case 'none':
      return '不提权'
    case 'sudoNonInteractive':
      return `sudo 免交互 → ${policy.runAs}`
    case 'sudoPassword':
      return `sudo 输入密码 → ${policy.runAs}`
  }
}

function commandText(command: ResolvedCommand): string {
  return command.mode === 'script'
    ? command.script.display
    : [command.executable.display, ...command.args.map((a) => a.display)].join(' ')
}

/** The main line of a task: what runs, what is copied, or what is asked. */
export function taskCommand(action: TaskAction): { text: string; sensitive: boolean } {
  switch (action.type) {
    case 'local':
      return { text: action.command.display, sensitive: action.command.sensitive }
    case 'upload':
      return {
        text: `${action.source.display} → ${action.target.display}`,
        sensitive: action.source.sensitive || action.target.sensitive,
      }
    case 'remote': {
      const c = action.command
      const sensitive =
        c.mode === 'script'
          ? c.script.sensitive
          : c.executable.sensitive || c.args.some((a) => a.sensitive)
      return { text: commandText(c), sensitive }
    }
    case 'confirm':
      return { text: action.prompt.display, sensitive: action.prompt.sensitive }
  }
}

const ENCODING = { utf8: 'UTF-8', gbk: 'GBK' }
const POLICY = { stop: '失败停止', continue: '失败继续', skipServer: '跳过该服务器' }

/**
 * Directory, shell, conflict policy, privilege, timeout and failure policy of a task, as short
 * facts. `programPath` names the shell's executable when it is known.
 */
export function taskDetails(
  task: PlannedTask,
  programPath?: (shell: Shell) => string | null | undefined,
): string[] {
  const out: string[] = []
  const a = task.action
  if ((a.type === 'local' || a.type === 'remote') && a.cwd) out.push(`目录 ${a.cwd.display}`)
  if (a.type === 'local') {
    const shell = a.shell === 'wsl' ? `WSL ${a.wslDistro ?? ''}` : shellLabel(a.shell)
    const path = programPath?.(a.shell)
    out.push(path === null ? `${shell}（本机未找到）` : path ? `${shell} · ${path}` : shell)
    if (a.outputEncoding !== 'auto') out.push(`输出编码 ${ENCODING[a.outputEncoding]}`)
  }
  if (a.type === 'upload')
    out.push(a.conflict === 'overwrite' ? '覆盖已有目标' : '目标已存在时报错')
  if (a.type === 'remote') out.push(privilegeLabel(a.privilege))
  if (task.timeoutSec !== null) out.push(`超时 ${task.timeoutSec} 秒`)
  if (a.type !== 'confirm') out.push(POLICY[task.failurePolicy])
  return out
}

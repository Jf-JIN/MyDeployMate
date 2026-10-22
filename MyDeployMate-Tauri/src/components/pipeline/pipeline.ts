import type { ExecutionOrder, Pipeline, Server, Step, StepPrivilege } from '@/ipc'

/** Steps (and whether the pipeline itself) use "跳过该服务器" (IXD-08). */
export function skipServerUsers(pipeline: Pipeline): { pipeline: boolean; steps: Step[] } {
  return {
    pipeline: pipeline.failurePolicy === 'skipServer',
    steps: pipeline.steps.filter((s) => s.failurePolicy === 'skipServer'),
  }
}

/** Replace every "跳过该服务器" with a policy that step-first order allows. */
export function clearSkipServer(pipeline: Pipeline) {
  if (pipeline.failurePolicy === 'skipServer') pipeline.failurePolicy = 'stop'
  for (const s of pipeline.steps) if (s.failurePolicy === 'skipServer') s.failurePolicy = 'inherit'
}

/** Move a step up or down; returns its new index (IXD-09). */
export function moveStep(steps: Step[], index: number, delta: -1 | 1): number {
  const to = index + delta
  if (index < 0 || to < 0 || to >= steps.length) return index
  const [step] = steps.splice(index, 1)
  if (step) steps.splice(to, 0, step)
  return to
}

/** After deleting `index`, select the next step, else the previous, else nothing (IXD-09). */
export function nextSelection(steps: Step[], index: number): string | null {
  return steps[index]?.id ?? steps[index - 1]?.id ?? null
}

/** Insert `expression` over the selection [start, end) and return the new caret (IXD-11). */
export function insertExpression(
  value: string,
  start: number,
  end: number,
  expression: string,
): { value: string; caret: number } {
  const a = Math.max(0, Math.min(start, value.length))
  const b = Math.max(a, Math.min(end, value.length))
  return { value: value.slice(0, a) + expression + value.slice(b), caret: a + expression.length }
}

/** Task order for the execution-order example: three steps on two servers (IXD-08). */
export function exampleSequence(order: ExecutionOrder): string[] {
  const steps = ['构建', '上传', '重启']
  const servers = ['A', 'B']
  if (order === 'serverFirst') {
    return [
      '构建（本地，一次）',
      ...servers.flatMap((s) => steps.slice(1).map((x) => `${x} → ${s}`)),
    ]
  }
  return ['构建（本地，一次）', ...steps.slice(1).flatMap((x) => servers.map((s) => `${x} → ${s}`))]
}

/** What a remote step actually runs with on one server (IXD-10: no blanket "all passwordless"). */
export function effectivePrivilege(step: StepPrivilege, server: Server): string {
  const policy = step.mode === 'inherit' ? server.privilege : step
  const fromServer = step.mode === 'inherit' ? '（服务器默认）' : ''
  switch (policy.mode) {
    case 'none':
      return `不提权${fromServer}`
    case 'sudoNonInteractive':
      return `sudo 免交互，以 ${policy.runAs} 运行${fromServer}`
    case 'sudoPassword':
      return `sudo 运行时输入密码，以 ${policy.runAs} 运行${fromServer}`
    default:
      return ''
  }
}

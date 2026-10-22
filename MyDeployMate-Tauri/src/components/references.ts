import type { Reference } from '@/ipc'

/** One line per reference for confirmation and blocking dialogs (IXD-14, IXD-20). */
export function describeReference(ref: Reference): string {
  switch (ref.kind) {
    case 'server':
      return `服务器 ${ref.serverName} 属于该环境`
    case 'projectDefault':
      return `项目 ${ref.projectName} 以它为默认环境`
    case 'targetBinding':
      return `项目 ${ref.projectName} 在 ${ref.environmentName} 中绑定了 ${ref.serverName}`
    case 'variableOverride':
      return `项目 ${ref.projectName} 在该环境中覆盖了变量 ${ref.variableName}`
  }
}

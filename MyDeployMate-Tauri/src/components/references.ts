import { translate } from '@/i18n'
import type { Reference } from '@/ipc'

/** One line per reference for confirmation and blocking dialogs (IXD-14, IXD-20). */
export function describeReference(ref: Reference): string {
  switch (ref.kind) {
    case 'server':
      return translate('references.serverInEnv', { server: ref.serverName })
    case 'projectDefault':
      return translate('references.projectDefaultEnv', { project: ref.projectName })
    case 'targetBinding':
      return translate('references.projectBinding', {
        project: ref.projectName,
        environment: ref.environmentName,
        server: ref.serverName,
      })
    case 'variableOverride':
      return translate('references.projectOverride', {
        project: ref.projectName,
        variable: ref.variableName,
      })
  }
}

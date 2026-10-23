import { translate } from '@/i18n'
import type { CredentialPurpose } from '@/ipc'

/** IXD-31: the title says which secret this is. */
export function credentialTitle(purpose: CredentialPurpose): string {
  switch (purpose.purpose) {
    case 'sshPassword':
      return translate('credential.title.sshPassword')
    case 'keyPassphrase':
      return translate('credential.title.keyPassphrase')
    case 'sudoPassword':
      return translate('credential.title.sudoPassword')
  }
}

/** Short description for lists, e.g. the saved credentials table. */
export function purposeLabel(purpose: CredentialPurpose): string {
  switch (purpose.purpose) {
    case 'sshPassword':
      return translate('credential.purpose.sshPassword')
    case 'keyPassphrase':
      return translate('credential.purpose.keyPassphrase')
    case 'sudoPassword':
      return translate('credential.purpose.sudoPassword', { runAs: purpose.runAs })
  }
}

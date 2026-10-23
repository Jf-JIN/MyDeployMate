import type { CredentialPurpose } from '@/ipc'

/** IXD-31: the title says which secret this is. */
export function credentialTitle(purpose: CredentialPurpose): string {
  switch (purpose.purpose) {
    case 'sshPassword':
      return 'SSH 登录密码'
    case 'keyPassphrase':
      return '解锁私钥'
    case 'sudoPassword':
      return 'sudo 授权'
  }
}

/** Short description for lists, e.g. the saved credentials table. */
export function purposeLabel(purpose: CredentialPurpose): string {
  switch (purpose.purpose) {
    case 'sshPassword':
      return 'SSH 登录密码'
    case 'keyPassphrase':
      return '私钥口令'
    case 'sudoPassword':
      return `sudo 密码（提权为 ${purpose.runAs}）`
  }
}

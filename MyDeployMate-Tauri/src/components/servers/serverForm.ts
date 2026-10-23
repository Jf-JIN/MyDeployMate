import { translate } from '@/i18n'
import type { KeyFormat } from '@/ipc'

/** The format line under the key file field (IXD-17), e.g. "OpenSSH · 已加密". */
export function describeKeyFormat(format: KeyFormat): string {
  const line = (name: string, encrypted: boolean) =>
    translate('servers.keyFormat.line', {
      format: name,
      lock: translate(encrypted ? 'servers.keyFormat.encrypted' : 'servers.keyFormat.unencrypted'),
    })
  switch (format.format) {
    case 'openSsh':
      return line('OpenSSH', format.encrypted)
    case 'pem':
      return line('PEM', format.encrypted)
    case 'putty':
      return line(`PuTTY v${format.version}`, format.encrypted)
    case 'unknown':
      return translate('servers.keyFormat.unknown')
  }
}

/** Tags typed as one line, split on spaces and commas; duplicates are dropped. */
export function parseTags(text: string): string[] {
  return [...new Set(text.split(/[\s,\uFF0C\u3001]+/).filter(Boolean))]
}

/** `host:port` for display; an IPv6 address is bracketed so the port stays readable. */
export function hostPort(host: string, port: number): string {
  return host.includes(':') ? `[${host}]:${port}` : `${host}:${port}`
}

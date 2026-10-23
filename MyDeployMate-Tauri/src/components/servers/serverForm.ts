import type { KeyFormat } from '@/ipc'

/** The format line under the key file field (IXD-17), e.g. "OpenSSH · 已加密". */
export function describeKeyFormat(format: KeyFormat): string {
  const lock = (encrypted: boolean) => (encrypted ? '已加密' : '未加密')
  switch (format.format) {
    case 'openSsh':
      return `OpenSSH · ${lock(format.encrypted)}`
    case 'pem':
      return `PEM · ${lock(format.encrypted)}`
    case 'putty':
      return `PuTTY v${format.version} · ${lock(format.encrypted)}`
    case 'unknown':
      return '无法识别的格式'
  }
}

/** Tags typed as one line, split on spaces and commas; duplicates are dropped. */
export function parseTags(text: string): string[] {
  return [...new Set(text.split(/[\s,，、]+/).filter(Boolean))]
}

/** `host:port` for display; an IPv6 address is bracketed so the port stays readable. */
export function hostPort(host: string, port: number): string {
  return host.includes(':') ? `[${host}]:${port}` : `${host}:${port}`
}

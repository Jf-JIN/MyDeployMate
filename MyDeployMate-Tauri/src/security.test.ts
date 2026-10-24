// Web security settings of the app window (SRS NFR-02; architecture ARC-36; plan task 8.3).
// A change here should be deliberate: loosening the CSP or granting a new plugin permission
// widens what injected script could do.
import { describe, expect, it } from 'vitest'

import confText from '../src-tauri/tauri.conf.json?raw'
import capabilityText from '../src-tauri/capabilities/default.json?raw'
import releaseText from '../src-tauri/tauri.release.conf.json?raw'

interface TauriConf {
  app: {
    withGlobalTauri?: boolean
    security: { csp: string; dangerousDisableAssetCspModification?: unknown }
  }
  bundle: { createUpdaterArtifacts?: boolean }
  plugins: { updater: Record<string, unknown> & { endpoints: string[]; pubkey: string } }
}

const conf = JSON.parse(confText) as TauriConf
const capability = JSON.parse(capabilityText) as { permissions: string[] }

function directives(csp: string): Map<string, string[]> {
  const map = new Map<string, string[]>()
  for (const part of csp.split(';')) {
    const [name, ...values] = part.trim().split(/\s+/)
    if (name) map.set(name, values)
  }
  return map
}

describe('content security policy', () => {
  const csp = directives(conf.app.security.csp)

  it('loads scripts only from the app itself', () => {
    expect(csp.get('default-src')).toEqual(["'self'"])
    // No script-src: scripts fall back to default-src, and Tauri adds nonces for its own.
    expect(csp.has('script-src')).toBe(false)
    expect(conf.app.security.csp).not.toMatch(/unsafe-eval|wasm-unsafe-eval|\*/)
    expect(conf.app.security.dangerousDisableAssetCspModification).toBeUndefined()
  })

  it('allows no remote origin, plugin object, frame, base or form target', () => {
    expect(csp.get('connect-src')).toEqual(['ipc:', 'http://ipc.localhost'])
    expect(csp.get('img-src')).toEqual(["'self'", 'data:'])
    for (const name of ['object-src', 'frame-src', 'base-uri', 'form-action']) {
      expect(csp.get(name), name).toEqual(["'none'"])
    }
    const origins = [...csp.values()].flat().filter((v) => /^https?:/.test(v))
    expect(origins).toEqual(['http://ipc.localhost'])
  })

  it('keeps the Tauri API off window', () => {
    expect(conf.app.withGlobalTauri ?? false).toBe(false)
  })
})

// NFR-04, ADR-11: updates come only from this repository's releases over HTTPS, and the
// installer must carry a signature for the version it announces.
describe('updater', () => {
  const updater = conf.plugins.updater

  it('reads releases of this repository over HTTPS only', () => {
    expect(updater.endpoints).toEqual([
      'https://github.com/Jf-JIN/MyDeployMate/releases/latest/download/latest.json',
    ])
    expect(Object.keys(updater).filter((k) => k.startsWith('dangerous'))).toEqual([])
  })

  it('verifies signatures and the signed version, and never downgrades', () => {
    expect(atob(updater.pubkey)).toMatch(/^untrusted comment: minisign public key: [0-9A-F]{16}\n/)
    expect(updater.requireSignedVersion).toBe(true)
    expect(updater.allowDowngrades ?? false).toBe(false)
  })

  it('creates update packages only in the release build, which has the key', () => {
    expect(conf.bundle.createUpdaterArtifacts ?? false).toBe(false)
    expect(JSON.parse(releaseText)).toMatchObject({ bundle: { createUpdaterArtifacts: true } })
  })
})

describe('window capability', () => {
  it('grants only the plugin permissions the app uses', () => {
    expect(capability.permissions).toEqual([
      'core:default',
      'core:window:allow-set-theme',
      'dialog:allow-open',
      'dialog:allow-save',
    ])
  })
})

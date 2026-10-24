// Web security settings of the app window (SRS NFR-02; architecture ARC-36; plan task 8.3).
// A change here should be deliberate: loosening the CSP or granting a new plugin permission
// widens what injected script could do.
import { describe, expect, it } from 'vitest'

import confText from '../src-tauri/tauri.conf.json?raw'
import capabilityText from '../src-tauri/capabilities/default.json?raw'

interface TauriConf {
  app: {
    withGlobalTauri?: boolean
    security: { csp: string; dangerousDisableAssetCspModification?: unknown }
  }
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

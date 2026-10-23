import { describe, expect, it } from 'vitest'

import { describeKeyFormat, hostPort, parseTags } from './serverForm'

describe('describeKeyFormat', () => {
  it('names the format and whether it has a passphrase', () => {
    expect(describeKeyFormat({ format: 'openSsh', encrypted: true })).toBe('OpenSSH · 已加密')
    expect(describeKeyFormat({ format: 'putty', version: 3, encrypted: false })).toBe(
      'PuTTY v3 · 未加密',
    )
    expect(describeKeyFormat({ format: 'unknown' })).toBe('无法识别的格式')
  })
})

describe('parseTags', () => {
  it('splits on spaces and commas and drops duplicates', () => {
    expect(parseTags(' api, web，api  db ')).toEqual(['api', 'web', 'db'])
    expect(parseTags('')).toEqual([])
  })
})

describe('hostPort', () => {
  it('brackets IPv6 addresses', () => {
    expect(hostPort('192.0.2.21', 22)).toBe('192.0.2.21:22')
    expect(hostPort('2001:db8::23', 22)).toBe('[2001:db8::23]:22')
  })
})

import { describe, expect, it } from 'vitest'

import type { Project, Variable } from '@/ipc'

import {
  clearOverride,
  describeSource,
  findOverride,
  removeVariable,
  renameVariable,
  setOverride,
} from './variables'

const literal = (value: string) => ({ kind: 'literal' as const, value })

function variable(id: string, name: string): Variable {
  return { id, name, value: literal('x'), sensitive: false }
}

function project(): Project {
  return {
    id: 'p',
    name: 'p',
    description: '',
    root: null,
    defaultEnvironmentId: 'staging',
    targets: [],
    variables: [variable('v1', 'port'), variable('v2', 'app_root')],
    overrides: [
      { environmentId: 'staging', name: 'port', value: literal('8082') },
      { environmentId: 'staging', name: 'app_root', value: literal('/srv') },
    ],
    pipelines: [],
  }
}

describe('overrides', () => {
  it('keeps an empty override distinct from no override', () => {
    const p = project()
    setOverride(p, 'production', 'port', literal(''))
    expect(findOverride(p, 'production', 'port')?.value).toEqual(literal(''))
    clearOverride(p, 'production', 'port')
    expect(findOverride(p, 'production', 'port')).toBeUndefined()
    expect(findOverride(p, 'staging', 'port')).toBeDefined()
  })

  it('carries overrides along on rename unless a global owns the name', () => {
    const p = project()
    renameVariable(p, [], 'port', 'http_port')
    expect(findOverride(p, 'staging', 'http_port')).toBeDefined()
    renameVariable(p, [variable('g', 'app_root')], 'app_root', 'root')
    expect(findOverride(p, 'staging', 'app_root')).toBeDefined()
  })

  it('drops overrides only the removed variable used', () => {
    const p = project()
    removeVariable(p, [variable('g', 'app_root')], 'v2')
    removeVariable(p, [], 'v1')
    expect(p.variables).toEqual([])
    expect(p.overrides.map((o) => o.name)).toEqual(['app_root'])
  })
})

describe('describeSource', () => {
  it('names environment variables and the credential manager', () => {
    expect(describeSource('project', literal('1'), 'staging')).toBe('项目')
    expect(describeSource('environmentOverride', { kind: 'envRef', name: 'PORT' }, 'staging')).toBe(
      'staging 覆盖 · 环境变量 PORT',
    )
    expect(describeSource('global', { kind: 'secretRef' }, 'staging')).toBe('全局 · 凭据管理器')
  })
})

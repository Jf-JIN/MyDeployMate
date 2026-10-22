import { describe, expect, it } from 'vitest'

import type { Project } from '@/ipc'

import { moveTarget, setTargetIds, targetIds } from './targets'

function project(): Project {
  return {
    id: 'p',
    name: 'p',
    description: '',
    root: null,
    defaultEnvironmentId: 'staging',
    targets: [{ environmentId: 'staging', serverIds: ['a', 'b'] }],
    variables: [],
    overrides: [],
    pipelines: [],
  }
}

describe('targets', () => {
  it('adds, replaces and drops environment entries', () => {
    const p = project()
    setTargetIds(p, 'production', ['c'])
    expect(targetIds(p, 'production')).toEqual(['c'])
    setTargetIds(p, 'staging', ['b'])
    expect(p.targets[0]).toEqual({ environmentId: 'staging', serverIds: ['b'] })
    setTargetIds(p, 'production', [])
    expect(p.targets).toEqual([{ environmentId: 'staging', serverIds: ['b'] }])
  })

  it('moves within bounds only', () => {
    expect(moveTarget(['a', 'b', 'c'], 'c', -1)).toEqual(['a', 'c', 'b'])
    expect(moveTarget(['a', 'b'], 'a', -1)).toEqual(['a', 'b'])
    expect(moveTarget(['a', 'b'], 'x', 1)).toEqual(['a', 'b'])
  })
})

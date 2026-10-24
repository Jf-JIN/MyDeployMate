import { describe, expect, it } from 'vitest'

import type { DiffEntry } from '@/ipc'

import { groupChanges } from './importDiff'

const e = (kind: DiffEntry['kind'], change: DiffEntry['change'], name = 'x'): DiffEntry => ({
  kind,
  change,
  name,
  parent: null,
})

describe('groupChanges', () => {
  it('groups by kind in page order and counts each change', () => {
    const groups = groupChanges([
      e('pipeline', 'added'),
      e('environment', 'removed'),
      e('pipeline', 'modified'),
      e('pipeline', 'added'),
    ])
    expect(groups.map((g) => g.kind)).toEqual(['environment', 'pipeline'])
    expect(groups[1]!.counts).toEqual({ added: 2, modified: 1, removed: 0 })
    expect(groups[1]!.entries).toHaveLength(3)
  })

  it('returns nothing for an identical file', () => {
    expect(groupChanges([])).toEqual([])
  })
})

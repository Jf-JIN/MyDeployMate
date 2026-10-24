import type { Change, DiffEntry, DiffKind } from '@/ipc'

const KIND_ORDER: DiffKind[] = [
  'environment',
  'server',
  'project',
  'pipeline',
  'targets',
  'globalVariable',
  'preferences',
]

export interface DiffGroup {
  kind: DiffKind
  counts: Record<Change, number>
  entries: DiffEntry[]
}

/** Import changes grouped by object kind, in page order, each with its change counts. */
export function groupChanges(changes: DiffEntry[]): DiffGroup[] {
  return KIND_ORDER.flatMap((kind) => {
    const entries = changes.filter((c) => c.kind === kind)
    if (!entries.length) return []
    const counts: Record<Change, number> = { added: 0, modified: 0, removed: 0 }
    for (const e of entries) counts[e.change] += 1
    return [{ kind, counts, entries }]
  })
}

import type { EnvironmentTargets, Project } from '@/ipc'

/** Bound servers of a project in an environment, in execution order (DATA-03). */
export function targetIds(project: Project, environmentId: string): string[] {
  return project.targets.find((t) => t.environmentId === environmentId)?.serverIds ?? []
}

/**
 * Replace the bound servers of one environment. An environment left with no servers drops
 * its entry, so binding and unbinding the same server leaves the draft unchanged.
 */
export function setTargetIds(project: Project, environmentId: string, ids: string[]) {
  const others = project.targets.filter((t) => t.environmentId !== environmentId)
  const entry: EnvironmentTargets = { environmentId, serverIds: ids }
  const at = project.targets.findIndex((t) => t.environmentId === environmentId)
  if (!ids.length) project.targets = others
  else if (at < 0) project.targets = [...others, entry]
  else project.targets.splice(at, 1, entry)
}

/** Move one bound server up or down in the execution order (IXD-12). */
export function moveTarget(ids: string[], id: string, delta: -1 | 1): string[] {
  const from = ids.indexOf(id)
  const to = from + delta
  if (from < 0 || to < 0 || to >= ids.length) return ids
  const next = [...ids]
  next.splice(from, 1)
  next.splice(to, 0, id)
  return next
}

import type { Project } from '@/ipc'

/** Target bindings of a project across all environments, shown before deleting it (IXD-06). */
export function boundTargetCount(project: Project): number {
  return project.targets.reduce((sum, t) => sum + t.serverIds.length, 0)
}

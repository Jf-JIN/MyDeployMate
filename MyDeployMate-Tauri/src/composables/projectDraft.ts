import { computed, type Ref } from 'vue'

import type { MessageKey } from '@/i18n'

import { useWorkspaceDraft } from './workspaceDraft'

/**
 * A project tab's draft (P03–P05): the whole workspace is drafted so Rust validates the
 * project in context, and issues are looked up relative to `projects[i]`.
 */
export function useProjectDraft(
  label: MessageKey,
  root: Ref<HTMLElement | null>,
  projectId: () => string,
) {
  const workspaceDraft = useWorkspaceDraft(label, root)
  const { draft, issues, at } = workspaceDraft

  const index = computed(() => draft.value?.projects.findIndex((p) => p.id === projectId()) ?? -1)
  const project = computed(() =>
    index.value < 0 ? null : (draft.value?.projects[index.value] ?? null),
  )
  const prefix = computed(() => `projects[${index.value}]`)

  /** Issue message at a path inside the project, e.g. `name` or `variables[2].name`. */
  function fieldError(field: string): string | null {
    return index.value < 0 ? null : at(`${prefix.value}.${field}`)
  }

  /** Issues that this page does not show next to a field. */
  function outside(shown: string[]) {
    return computed(() =>
      issues.value.filter((i) => !shown.some((s) => i.path.startsWith(`${prefix.value}.${s}`))),
    )
  }

  return { ...workspaceDraft, index, project, prefix, fieldError, outside }
}
